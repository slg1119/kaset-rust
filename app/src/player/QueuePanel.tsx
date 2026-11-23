import { useRef } from "react";
import { commands, type QueueState } from "../bindings";
import { RemoveIcon } from "../icons";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { SongMenu } from "../music/SongMenu";

type Props = {
  id: string;
  queue: QueueState | null;
  onOpenArtist?: OpenArtist;
  run: (action: () => Promise<unknown>) => void;
};

export function QueuePanel({ id, queue, onOpenArtist, run }: Props) {
  const drag = useRef<{ queue: QueueState | null; index: number } | null>(null);
  const current = queue?.current ?? -1;
  const songs = queue?.songs ?? [];
  const occurrences = new Map<string, number>();

  function move(from: number, to: number) {
    if (from > current && to > current && to < songs.length && from !== to) {
      run(() => commands.moveQueueItem(from, to));
    }
  }

  return (
    <section id={id} className="queue-panel" aria-label="Queue">
      <div className="queue-header">
        <button
          type="button"
          className="ghost"
          aria-label="Clear upcoming"
          disabled={current >= songs.length - 1}
          onClick={() => run(() => commands.clearUpcoming())}
        >
          Clear
        </button>
      </div>
      <ol>
        {songs.map((song, index) => {
          const occurrence = occurrences.get(song.video_id) ?? 0;
          occurrences.set(song.video_id, occurrence + 1);
          const upcoming = index > current;
          const isCurrent = index === current;
          const removeLabel = `Remove ${song.title}`;
          return (
            <li
              key={`${song.video_id}-${occurrence}`}
              aria-current={isCurrent ? "true" : undefined}
              draggable={upcoming}
              onDragStart={(event) => {
                if (!upcoming) return;
                drag.current = { queue, index };
                event.dataTransfer.effectAllowed = "move";
                event.dataTransfer.setData("text/plain", song.title);
              }}
              onDragEnd={() => { drag.current = null; }}
              onDragOver={(event) => {
                if (!upcoming || drag.current?.queue !== queue) return;
                event.preventDefault();
                event.dataTransfer.dropEffect = "move";
              }}
              onDrop={(event) => {
                const source = drag.current;
                drag.current = null;
                if (!upcoming || !source || source.queue !== queue) return;
                event.preventDefault();
                move(source.index, index);
              }}
            >
              <SongMenu song={song} className="queue-track item-with-links" onOpenArtist={onOpenArtist}>
                <button
                  type="button"
                  className="item-action"
                  aria-label={song.title}
                  aria-keyshortcuts={upcoming ? "Alt+ArrowUp Alt+ArrowDown" : undefined}
                  onClick={() => run(() => commands.jumpTo(index))}
                  onKeyDown={(event) => {
                    if (!upcoming || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
                    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
                    event.preventDefault();
                    move(index, index + (event.key === "ArrowUp" ? -1 : 1));
                  }}
                />
                <span className="item-text">
                  <span className="item-title">{song.title}</span>
                  <span className="item-subtitle"><ArtistNames artists={song.artists} onOpenArtist={onOpenArtist} /></span>
                </span>
                {upcoming && (
                  <button
                    type="button"
                    className="icon-button queue-remove"
                    aria-label={removeLabel}
                    title={removeLabel}
                    onClick={() => run(() => commands.removeQueueItem(index))}
                  >
                    <RemoveIcon size={16} />
                  </button>
                )}
              </SongMenu>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
