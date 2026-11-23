import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { commands, type LyricLine } from "../bindings";
import { errorMessage } from "../errors";
import { LYRICS_STALE_MS, queryKeys } from "../query";

type LyricsPanelProps = {
  id: string;
  videoId: string;
  position: number | null;
  canSeek: boolean;
  onSeek: (position: number) => void;
};

export function LyricsPanel({ id, videoId, position, canSeek, onSeek }: LyricsPanelProps) {
  const query = useQuery({
    queryKey: queryKeys.lyrics(videoId),
    queryFn: () => commands.getLyrics(videoId),
    staleTime: LYRICS_STALE_MS,
  });

  return (
    <section id={id} className="lyrics-panel" aria-label="Lyrics" tabIndex={0}>
      {query.isPending ? (
        <p role="status">Loading lyrics…</p>
      ) : query.isError ? (
        <div role="alert">
          <p>Could not load lyrics. {errorMessage(query.error)}</p>
          <button className="ghost lyrics-retry" type="button" disabled={query.isFetching} onClick={() => void query.refetch()}>
            Retry
          </button>
        </div>
      ) : (
        <>
          {query.data.lines?.length ? (
            <TimedLyrics lines={query.data.lines} position={position} canSeek={canSeek} onSeek={onSeek} />
          ) : query.data.text ? (
            <p className="lyrics-text">{query.data.text}</p>
          ) : (
            <p>No lyrics</p>
          )}
          {query.data.source && <p className="lyrics-source">{query.data.source}</p>}
        </>
      )}
    </section>
  );
}

function TimedLyrics({
  lines,
  position,
  canSeek,
  onSeek,
}: {
  lines: LyricLine[];
  position: number | null;
  canSeek: boolean;
  onSeek: (position: number) => void;
}) {
  const positionMs = position === null ? -1 : position * 1000;
  let candidate = lines.length - 1;
  while (candidate >= 0 && lines[candidate].start_ms > positionMs) candidate--;
  const end = lines[candidate]?.end_ms;
  const current = end != null && positionMs >= end ? -1 : candidate;
  const activeLine = useRef<HTMLLIElement>(null);

  useEffect(() => {
    if (current < 0) return;
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    activeLine.current?.scrollIntoView?.({
      block: "center",
      behavior: reducedMotion ? "instant" : "smooth",
    });
  }, [current, lines]);

  return (
    <ol className="lyrics-lines">
      {lines.map((line, index) => (
        <li key={`${line.start_ms}-${index}`} ref={index === current ? activeLine : undefined}>
          {line.text ? (
            <button
              type="button"
              aria-current={index === current ? "true" : undefined}
              disabled={!canSeek}
              onClick={() => onSeek(line.start_ms / 1000)}
            >
              {line.text}
            </button>
          ) : (
            <span className="lyrics-gap" aria-hidden="true" />
          )}
        </li>
      ))}
    </ol>
  );
}
