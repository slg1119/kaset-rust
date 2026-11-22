import { useState } from "react";
import { commands, type RepeatMode } from "../bindings";
import { errorMessage } from "../errors";
import { formatTime } from "./format";
import { usePlayer } from "./usePlayer";

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: "all", all: "one", one: "off" };
const REPEAT_LABEL: Record<RepeatMode, string> = {
  off: "Repeat off",
  all: "Repeat all",
  one: "Repeat one",
};

/** Now playing, transport controls, and progress. Hidden while nothing is queued. */
export function PlayerBar() {
  const { state, queue } = usePlayer();
  const [error, setError] = useState<string | null>(null);
  // While the user drags the seek slider, show the drag position.
  const [dragPosition, setDragPosition] = useState<number | null>(null);

  if (state === null || state.track === null) return null;

  function run(action: () => Promise<unknown>) {
    setError(null);
    action().catch((cause: unknown) => setError(errorMessage(cause)));
  }

  function commitSeek() {
    if (dragPosition === null) return;
    const position = dragPosition;
    setDragPosition(null);
    run(() => commands.seekTo(position));
  }

  const track = state.track;
  const artists = track.artists.map((artist) => artist.name).join(", ");
  const playing = state.status === "playing" || state.status === "loading";
  const duration = state.duration ?? 0;
  const position = dragPosition ?? state.position ?? 0;

  return (
    <footer className="player-bar" aria-label="Player">
      <div className="player-track">
        <strong>{track.title}</strong>
        {artists && <span>{artists}</span>}
        {queue && queue.current !== null && (
          <span className="player-queue">
            {queue.current + 1} of {queue.songs.length}
          </span>
        )}
      </div>
      <div className="player-controls">
        <button
          type="button"
          aria-pressed={state.shuffle}
          onClick={() => run(() => commands.setShuffle(!state.shuffle))}
        >
          Shuffle
        </button>
        <button type="button" onClick={() => run(() => commands.previousTrack())}>
          Previous
        </button>
        <button type="button" onClick={() => run(() => commands.togglePlayback())}>
          {playing ? "Pause" : "Play"}
        </button>
        <button
          type="button"
          disabled={!state.has_next}
          onClick={() => run(() => commands.nextTrack())}
        >
          Next
        </button>
        <button type="button" onClick={() => run(() => commands.setRepeat(NEXT_REPEAT[state.repeat]))}>
          {REPEAT_LABEL[state.repeat]}
        </button>
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
              min={0}
              max={duration}
              step={1}
              value={Math.min(position, duration)}
              disabled={duration <= 0}
              onChange={(event) => setDragPosition(Number(event.target.value))}
              onPointerUp={commitSeek}
              onKeyUp={commitSeek}
            />
            <span>{formatTime(duration)}</span>
          </>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
    </footer>
  );
}
