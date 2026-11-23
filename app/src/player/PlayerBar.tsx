import { useEffect, useId, useRef, useState } from "react";
import { commands, type RepeatMode } from "../bindings";
import { errorMessage } from "../errors";
import { LyricsIcon, NextIcon, PauseIcon, PlayIcon, PreviousIcon, QueueIcon, RepeatIcon, RepeatOneIcon, ShuffleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { useNotice } from "../notices/NoticeProvider";
import { useShortcuts } from "../shell/useShortcuts";
import { LyricsPanel } from "./LyricsPanel";
import { formatTime } from "./format";
import { QueuePanel } from "./QueuePanel";
import { TrackRating } from "./TrackRating";
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
 * the middle, shuffle/repeat/queue/lyrics on the right. Hidden while nothing is
 * queued. Publishes its height as `--player-space` on its parent so the
 * scrolling content can leave room below the last row.
 */
export function PlayerBar({ onOpenArtist }: { onOpenArtist?: OpenArtist }) {
  const { report, dismiss } = useNotice();
  const { state, queue } = usePlayer();
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
  const [openPanel, setOpenPanel] = useState<"queue" | "lyrics" | null>(null);
  const queueOpen = openPanel === "queue";
  const lyricsOpen = openPanel === "lyrics";
  const settleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const bar = useRef<HTMLElement>(null);
  const queueButton = useRef<HTMLButtonElement>(null);
  const queueId = useId();
  const lyricsButton = useRef<HTMLButtonElement>(null);
  const lyricsId = useId();
  const queuePositionId = useId();
  const queuePosition = queue?.current != null ? `${queue.current + 1} of ${queue.songs.length}` : null;
  const visible = state !== null && state.track !== null;

  function toggleLyrics() {
    setOpenPanel((panel) => panel === "lyrics" ? null : "lyrics");
  }

  useShortcuts({ toggleLyrics: visible ? toggleLyrics : undefined });

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
    if (openPanel === null || !visible) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape" || event.isComposing || event.defaultPrevented) return;
      if (event.target instanceof Element && event.target.closest(".song-menu, dialog[open]")) return;
      // Capture phase + preventDefault: closing a panel must not also trigger the Back shortcut.
      event.preventDefault();
      setOpenPanel(null);
      (openPanel === "queue" ? queueButton : lyricsButton).current?.focus();
    }
    function onPointerDown(event: PointerEvent) {
      if (event.target instanceof Element && event.target.closest(".song-menu, dialog[open]")) return;
      if (queueOpen && event.target instanceof Node && !bar.current?.contains(event.target)) {
        setOpenPanel(null);
      }
    }
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("pointerdown", onPointerDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("pointerdown", onPointerDown);
    };
  }, [openPanel, queueOpen, visible]);

  if (state === null || state.track === null) return null;
  const track = state.track;

  function run(action: () => Promise<unknown>) {
    dismiss();
    action().catch((cause: unknown) => report(errorMessage(cause)));
  }

  function commitSeek() {
    if (dragPosition === null) return;
    seek(dragPosition);
  }

  function seek(target: number) {
    setDragPosition(null);
    setPendingSeek({ videoId: track.video_id, target });
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => setPendingSeek(null), SEEK_SETTLE_MS);
    run(() => commands.seekTo(target));
  }

  const playing = state.status === "playing" || state.status === "loading";
  const playbackLabel = playing ? "Pause" : "Play";
  const repeatLabel = REPEAT_LABEL[state.repeat];
  const duration = state.duration ?? 0;
  const reported = state.position ?? 0;
  const seekTarget = pendingSeek?.videoId === track.video_id ? pendingSeek.target : null;
  const settled = seekTarget === null || Math.abs(reported - seekTarget) <= SEEK_TOLERANCE_SECONDS;
  const position = dragPosition ?? (settled ? reported : seekTarget);

  const playerBar = (
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
          disabled={!state.has_next && state.status !== "ended"}
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
            {track.artists.length > 0 && (
              <span className="item-subtitle"><ArtistNames artists={track.artists} onOpenArtist={onOpenArtist} /></span>
            )}
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
        {!state.is_ad && <TrackRating key={track.video_id} videoId={track.video_id} />}
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
          aria-describedby={queuePosition ? queuePositionId : undefined}
          onClick={() => setOpenPanel((panel) => panel === "queue" ? null : "queue")}
        >
          <QueueIcon />
          {queuePosition && <span id={queuePositionId} className="player-queue">{queuePosition}</span>}
        </button>
        <button
          type="button"
          className={lyricsOpen ? "icon-button is-active" : "icon-button"}
          ref={lyricsButton}
          aria-label="Lyrics"
          title="Lyrics"
          aria-expanded={lyricsOpen}
          aria-controls={lyricsId}
          aria-keyshortcuts="Meta+Shift+L Control+Shift+L"
          onClick={toggleLyrics}
        >
          <LyricsIcon />
        </button>
      </div>
      {queueOpen && (
        <QueuePanel
          id={queueId}
          queue={queue}
          run={run}
          onOpenArtist={onOpenArtist && ((browseId, name) => {
            setOpenPanel(null);
            onOpenArtist(browseId, name);
          })}
        />
      )}
    </footer>
  );

  // Keep the lyrics backdrop outside the player bar’s own blur.
  return (
    <>
      {playerBar}
      {lyricsOpen && (
        <LyricsPanel
          key={track.video_id}
          id={lyricsId}
          videoId={track.video_id}
          position={state.is_ad ? null : settled ? state.position : seekTarget}
          canSeek={!state.is_ad}
          onSeek={seek}
        />
      )}
    </>
  );
}
