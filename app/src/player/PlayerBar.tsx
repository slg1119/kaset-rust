import { useEffect, useId, useRef, useState } from "react";
import { commands, type RepeatMode } from "../bindings";
import { errorMessage } from "../errors";
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
  const [pendingSeek, setPendingSeek] = useState<number | null>(null);
  const [queueOpen, setQueueOpen] = useState(false);
  const settleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const bar = useRef<HTMLElement>(null);
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

  if (state === null || state.track === null) return null;

  function run(action: () => Promise<unknown>) {
    dismiss();
    action().catch((cause: unknown) => report(errorMessage(cause)));
  }

  function commitSeek() {
    if (dragPosition === null) return;
    const target = dragPosition;
    setDragPosition(null);
    setPendingSeek(target);
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => setPendingSeek(null), SEEK_SETTLE_MS);
    run(() => commands.seekTo(target));
  }

  const track = state.track;
  const artists = artistNames(track.artists);
  const playing = state.status === "playing" || state.status === "loading";
  const duration = state.duration ?? 0;
  const reported = state.position ?? 0;
  const settled = pendingSeek === null || Math.abs(reported - pendingSeek) <= SEEK_TOLERANCE_SECONDS;
  const position = dragPosition ?? (settled ? reported : pendingSeek);

  return (
    <footer ref={bar} className="player-bar" aria-label="Player">
      <div className="player-controls">
        <button type="button" className="icon-button" onClick={() => run(() => commands.previousTrack())}>
          Previous
        </button>
        <button type="button" className="play-button" onClick={() => run(() => commands.togglePlayback())}>
          {playing ? "Pause" : "Play"}
        </button>
        <button
          type="button"
          className="icon-button"
          disabled={!state.has_next}
          onClick={() => run(() => commands.nextTrack())}
        >
          Next
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
          aria-pressed={state.shuffle}
          onClick={() => run(() => commands.setShuffle(!state.shuffle))}
        >
          Shuffle
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={() => run(() => commands.setRepeat(NEXT_REPEAT[state.repeat]))}
        >
          {REPEAT_LABEL[state.repeat]}
        </button>
        <button
          type="button"
          className="icon-button"
          aria-expanded={queueOpen}
          aria-controls={queueId}
          onClick={() => setQueueOpen((open) => !open)}
        >
          Queue
          {queue && queue.current !== null && (
            <span className="player-queue">
              {" "}
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
