import { useEffect, useId, useRef, useState } from "react";
import { commands, type RepeatMode } from "../bindings";
import { errorMessage } from "../errors";
import { NextIcon, PauseIcon, PlayIcon, PreviousIcon, QueueIcon, RepeatIcon, RepeatOneIcon, ShuffleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { artistNames } from "../music/items";
import { useNotice } from "../notices/NoticeProvider";
import { formatTime } from "./format";
import { usePlayer } from "./usePlayer";

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: "all", all: "one", one: "off" };
const REPEAT_LABEL: Record<RepeatMode, string> = {
  off: "Repeat off",
  all: "Repeat all",
  one: "Repeat one",
};

/** After a seek, keep showing the target until the page reports a position this close… */
const SEEK_TOLERANCE_SECONDS = 1.5;
/** …or until this much time has passed. */
const SEEK_SETTLE_MS = 2000;

/**
 * The floating player bar: transport on the left, now playing with progress in
 * the middle, shuffle/repeat/queue on the right. Hidden while nothing is
 * queued. Publishes its height as `--player-space` on its parent so the
 * scrolling content can leave room below the last row.
 */
export function PlayerBar() {
  const { report, dismiss } = useNotice();
  const { state, queue } = usePlayer(report);
  // While the user drags the seek slider, show the drag position.
  const [dragPosition, setDragPosition] = useState<number | null>(null);
  // After a seek, progress sampled before the page applied it must not snap the slider back.
  const [pendingSeek, setPendingSeek] = useState<{ videoId: string; target: number } | null>(null);
  const videoId = state?.track?.video_id;
  const [previousVideoId, setPreviousVideoId] = useState(videoId);
  if (previousVideoId !== videoId) {
    setPreviousVideoId(videoId);
    setDragPosition(null);
    setPendingSeek(null);
  }
  const [queueOpen, setQueueOpen] = useState(false);
  const settleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const bar = useRef<HTMLElement>(null);
  const queueButton = useRef<HTMLButtonElement>(null);
  const queueId = useId();
  const visible = state !== null && state.track !== null;

  useEffect(() => () => clearTimeout(settleTimer.current), []);

  useEffect(() => {
    const element = bar.current;
    const column = element?.parentElement;
    if (!element || !column || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() =>
      column.style.setProperty("--player-space", `${element.offsetHeight}px`),
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      column.style.removeProperty("--player-space");
    };
  }, [visible]);

  useEffect(() => {
    if (!queueOpen || !visible) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape" || event.isComposing || event.defaultPrevented) return;
      // Capture phase + preventDefault: closing the queue must not also trigger the Back shortcut.
      event.preventDefault();
      setQueueOpen(false);
      queueButton.current?.focus();
    }
    function onPointerDown(event: PointerEvent) {
      if (event.target instanceof Node && !bar.current?.contains(event.target)) {
        setQueueOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("pointerdown", onPointerDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("pointerdown", onPointerDown);
    };
  }, [queueOpen, visible]);

  if (state === null || state.track === null) return null;
  const track = state.track;

  function run(action: () => Promise<unknown>) {
    dismiss();
    action().catch((cause: unknown) => report(errorMessage(cause)));
  }

  function commitSeek() {
    if (dragPosition === null) return;
    const target = dragPosition;
    setDragPosition(null);
    setPendingSeek({ videoId: track.video_id, target });
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => setPendingSeek(null), SEEK_SETTLE_MS);
    run(() => commands.seekTo(target));
  }

  const artists = artistNames(track.artists);
  const playing = state.status === "playing" || state.status === "loading";
  const playbackLabel = playing ? "Pause" : "Play";
  const repeatLabel = REPEAT_LABEL[state.repeat];
  const duration = state.duration ?? 0;
  const reported = state.position ?? 0;
  const seekTarget = pendingSeek?.videoId === track.video_id ? pendingSeek.target : null;
  const settled = seekTarget === null || Math.abs(reported - seekTarget) <= SEEK_TOLERANCE_SECONDS;
  const position = dragPosition ?? (settled ? reported : seekTarget);

  return (
    <footer ref={bar} className="player-bar" aria-label="Player">
      <div className="player-controls">
        <button
          type="button"
          className="icon-button"
          aria-label="Previous"
          title="Previous"
          onClick={() => run(() => commands.previousTrack())}
        >
          <PreviousIcon />
        </button>
        <button
          type="button"
          className="play-button"
          aria-label={playbackLabel}
          title={playbackLabel}
          onClick={() => run(() => commands.togglePlayback())}
        >
          {playing ? <PauseIcon size={22} /> : <PlayIcon size={22} />}
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Next"
          title="Next"
          disabled={!state.has_next}
          onClick={() => run(() => commands.nextTrack())}
        >
          <NextIcon />
        </button>
      </div>
      <div className="player-now">
        <Artwork url={track.thumbnail_url} size={44} />
        <div className="player-meta">
          <div className="player-title">
            <strong className="item-title">{track.title}</strong>
            {artists && <span className="item-subtitle">{artists}</span>}
          </div>
          <div className="player-progress">
            {state.is_ad ? (
              <span className="player-ad">Ad</span>
            ) : (
              <>
                <span>{formatTime(position)}</span>
                <input
                  type="range"
                  aria-label="Seek"
                  aria-valuetext={`${formatTime(position)} of ${formatTime(duration)}`}
                  min={0}
                  max={duration}
                  step={1}
                  value={Math.min(position, duration)}
                  disabled={duration <= 0}
                  onChange={(event) => setDragPosition(Number(event.target.value))}
                  onPointerUp={commitSeek}
                  onKeyUp={commitSeek}
                  // A cancelled drag or focus loss must not leave the slider stuck on the drag value.
                  onPointerCancel={() => setDragPosition(null)}
                  onBlur={() => setDragPosition(null)}
                />
                <span>{formatTime(duration)}</span>
              </>
            )}
          </div>
        </div>
      </div>
      <div className="player-extras">
        <button
          type="button"
          className="icon-button"
          aria-label="Shuffle"
          title="Shuffle"
          aria-pressed={state.shuffle}
          onClick={() => run(() => commands.setShuffle(!state.shuffle))}
        >
          <ShuffleIcon />
        </button>
        <button
          type="button"
          className={state.repeat === "off" ? "icon-button" : "icon-button is-active"}
          aria-label={repeatLabel}
          title={repeatLabel}
          onClick={() => run(() => commands.setRepeat(NEXT_REPEAT[state.repeat]))}
        >
          {state.repeat === "one" ? <RepeatOneIcon /> : <RepeatIcon />}
        </button>
        <button
          type="button"
          className="icon-button"
          ref={queueButton}
          aria-label="Queue"
          title="Queue"
          aria-expanded={queueOpen}
          aria-controls={queueId}
          onClick={() => setQueueOpen((open) => !open)}
        >
          <QueueIcon />
          {queue && queue.current !== null && (
            <span className="player-queue">
              {queue.current + 1} of {queue.songs.length}
            </span>
          )}
        </button>
      </div>
      {queueOpen && (
        <section id={queueId} className="queue-panel" aria-label="Queue">
          <ol>
            {(queue?.songs ?? []).map((song, index) => (
              <li key={`${index}-${song.video_id}`} aria-current={index === queue?.current ? "true" : undefined}>
                <span className="item-title">{song.title}</span>
                <span className="item-subtitle">{artistNames(song.artists)}</span>
              </li>
            ))}
          </ol>
        </section>
      )}
    </footer>
  );
}
