import { useQuery } from "@tanstack/react-query";
import { useId, useRef } from "react";
import { commands, type HomeSection, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { ItemCard } from "../music/ItemCard";
import { isPlayable, itemKey, shapeOf, subtitleOf, toSong } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { HOME_STALE_MS, queryKeys } from "../query";

type PlayFn = (songs: Song[], index: number) => Promise<void>;

export function HomeScreen() {
  const home = useQuery({
    queryKey: queryKeys.home,
    queryFn: () => commands.getHome(),
    staleTime: HOME_STALE_MS,
  });
  const play = usePlayQueue();
  const sections = home.data?.sections ?? [];
  // The first all-song shelf (YouTube Music's "Quick picks") is a song grid.
  const gridIndex = sections.findIndex((section) => section.items.length > 0 && section.items.every(isPlayable));

  return (
    <>
      <header className="content-header">
        <h1>Home</h1>
        <button type="button" className="ghost" onClick={() => void home.refetch()} disabled={home.isFetching}>
          Refresh
        </button>
      </header>
      {home.isPending && <p className="hint" role="status">Loading…</p>}
      {home.isError && (
        // An auth_expired error also arrives as auth://changed, which replaces the shell.
        <div role="alert" className="screen-error">
          <p>{errorMessage(home.error)}</p>
          <button type="button" onClick={() => void home.refetch()}>
            Retry
          </button>
        </div>
      )}
      {home.data && sections.every((section) => section.items.length === 0) && <p className="hint">Nothing to show yet.</p>}
      {sections.map((section, index) => {
        if (section.items.length === 0) return null;
        return index === gridIndex ? (
          <SongGrid key={`${index}-${section.title}`} section={section} onPlay={play} />
        ) : (
          <Carousel key={`${index}-${section.title}`} section={section} onPlay={play} />
        );
      })}
    </>
  );
}

/** Songs of a shelf, and for each item its index among them (-1 for non-songs). */
function playOrder(section: HomeSection): { songs: Song[]; starts: number[] } {
  const songs = section.items.filter(isPlayable).map(toSong);
  let next = 0;
  const starts = section.items.map((item) => (isPlayable(item) ? next++ : -1));
  return { songs, starts };
}

type ShelfProps = { section: HomeSection; onPlay: PlayFn };

/** Quick picks: a three-column grid of song rows. */
function SongGrid({ section, onPlay }: ShelfProps) {
  const headingId = useId();
  const { songs, starts } = playOrder(section);
  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header">
        <h2 id={headingId}>{section.title}</h2>
      </div>
      <ul className="song-grid">
        {section.items.map((item, index) => (
          <li key={`${index}-${itemKey(item)}`}>
            <ItemCard
              layout="row"
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onPlay={() => void onPlay(songs, starts[index])}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** A shelf as a six-column carousel with previous/next buttons. */
function Carousel({ section, onPlay }: ShelfProps) {
  const headingId = useId();
  const list = useRef<HTMLUListElement>(null);
  const { songs, starts } = playOrder(section);

  function page(direction: 1 | -1) {
    const element = list.current;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    element?.scrollBy?.({ left: direction * element.clientWidth, behavior: reducedMotion ? "auto" : "smooth" });
  }

  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header">
        <h2 id={headingId}>{section.title}</h2>
        <div className="shelf-nav">
          <button type="button" className="icon-button" aria-label={`Previous in ${section.title}`} onClick={() => page(-1)}>
            ‹
          </button>
          <button type="button" className="icon-button" aria-label={`Next in ${section.title}`} onClick={() => page(1)}>
            ›
          </button>
        </div>
      </div>
      <ul ref={list} className="carousel">
        {section.items.map((item, index) => (
          <li key={`${index}-${itemKey(item)}`}>
            <ItemCard
              layout="card"
              shape={shapeOf(item)}
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onPlay={starts[index] >= 0 ? () => void onPlay(songs, starts[index]) : undefined}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}
