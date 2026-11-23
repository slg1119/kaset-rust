import { useLayoutEffect, useRef } from "react";
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
  const rowButtons = useRef(new Map<string, HTMLButtonElement>());
  const pendingFocus = useRef<{ key: string; source: HTMLButtonElement } | null>(null);
  const current = queue?.current ?? null;
  const songs = queue?.songs ?? [];
  const occurrences = new Map<string, number>();

  useLayoutEffect(() => {
    const focus = pendingFocus.current;
    if (!focus) return;
    pendingFocus.current = null;
    if (document.activeElement === focus.source || document.activeElement === document.body) {
      rowButtons.current.get(focus.key)?.focus();
    }
  }, [queue]);

  function move(from: number, to: number, focus?: { key: string; source: HTMLButtonElement }) {
    if (current === null || from <= current || to <= current || to >= songs.length || from === to) return;
    if (focus) pendingFocus.current = focus;
    run(async () => {
      try {
        await commands.moveQueueItem(from, to);
      } catch (error) {
        if (pendingFocus.current === focus) pendingFocus.current = null;
        throw error;
      }
    });
  }

  return (
    <section id={id} className="queue-panel" aria-label="Queue">
      <div className="queue-header">
        <button
          type="button"
          className="ghost"
          aria-label="Clear upcoming"
          disabled={current === null || current >= songs.length - 1}
          onClick={() => run(() => commands.clearUpcoming())}
        >
          Clear
        </button>
      </div>
      <ol>
        {songs.map((song, index) => {
          const occurrence = occurrences.get(song.video_id) ?? 0;
          occurrences.set(song.video_id, occurrence + 1);
          const upcoming = current !== null && index > current;
          const key = `${song.video_id}-${occurrence}`;
          const isCurrent = index === current;
          const removeLabel = `Remove ${song.title}`;
          return (
            <li
              key={key}
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
                  ref={(button) => {
                    if (button) rowButtons.current.set(key, button);
                    else rowButtons.current.delete(key);
                  }}
                  className="item-action"
                  aria-label={song.title}
                  aria-keyshortcuts={upcoming ? "Alt+ArrowUp Alt+ArrowDown" : undefined}
                  onClick={() => run(() => commands.jumpTo(index))}
                  onKeyDown={(event) => {
                    if (!upcoming || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
                    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
                    event.preventDefault();
                    const direction = event.key === "ArrowUp" ? -1 : 1;
                    const to = index + direction;
                    // Crossing another occurrence of this video changes the moved row's key.
                    const targetOccurrence = songs[to]?.video_id === song.video_id
                      ? occurrence + direction
                      : occurrence;
                    move(index, to, {
                      key: `${song.video_id}-${targetOccurrence}`,
                      source: event.currentTarget,
                    });
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
