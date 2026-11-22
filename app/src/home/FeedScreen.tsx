import { skipToken, useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { commands, type FeedKind, type HomeSection, type ShelfItem, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { ChevronLeftIcon, ChevronRightIcon, RefreshIcon } from "../icons";
import { ItemCard } from "../music/ItemCard";
import { hasBrowsePage, isPlayable, itemKey, shapeOf, subtitleOf, toSong, type OpenableItem } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { FEED_STALE_MS, nextPageParam, queryKeys } from "../query";

type PlayFn = (songs: Song[], index: number) => Promise<void>;

export type CategoryItem = Extract<ShelfItem, { kind: "category" }>;
export type FeedSource = { kind: FeedKind | "podcasts" } | { kind: "category"; browseId: string; params: string | null };

type Props = {
  source: FeedSource;
  title: string;
  onOpenPage: (card: OpenableItem) => void;
  onOpenCategory: (item: CategoryItem) => void;
  onBack?: () => void;
};

function useFeed(source: FeedSource, chip: string | null) {
  const isCategory = source.kind === "category";
  const base = useInfiniteQuery({
    queryKey: isCategory ? queryKeys.feedCategory(source.browseId, source.params) : queryKeys.feed(source.kind),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => {
      if (pageParam !== null) return commands.getFeedContinuation(pageParam);
      if (isCategory) return commands.getFeedCategory(source.browseId, source.params);
      return source.kind === "podcasts" ? commands.getPodcasts() : commands.getFeed(source.kind);
    },
    getNextPageParam: nextPageParam,
    staleTime: source.kind === "history" ? 0 : FEED_STALE_MS,
    enabled: chip === null,
  });
  const filtered = useInfiniteQuery({
    queryKey: queryKeys.feedChip(chip),
    initialPageParam: null as string | null,
    queryFn: chip === null ? skipToken : ({ pageParam }) => pageParam === null
      ? commands.getFeedChip(chip)
      : commands.getFeedContinuation(pageParam),
    getNextPageParam: nextPageParam,
    staleTime: FEED_STALE_MS,
  });
  return { feed: chip === null ? base : filtered, chips: base.data?.pages[0].chips ?? [] };
}

export function FeedScreen({ source, title, onOpenPage, onOpenCategory, onBack }: Props) {
  const [chip, setChip] = useState<string | null>(null);
  const { feed, chips } = useFeed(source, source.kind === "home" ? chip : null);
  const { fetchNextPage, hasNextPage, isFetching, isFetchNextPageError } = feed;
  const play = usePlayQueue();
  const sentinel = useRef<HTMLDivElement>(null);
  const backButton = useRef<HTMLButtonElement>(null);
  const sections = feed.data?.pages.flatMap((page) => page.sections) ?? [];
  const gridIndex = source.kind === "home"
    ? sections.findIndex((section) => section.items.length > 0 && section.items.every(isPlayable))
    : -1;

  useEffect(() => {
    backButton.current?.focus();
  }, []);

  useEffect(() => {
    const target = sentinel.current;
    if (!target || !hasNextPage || isFetching || isFetchNextPageError || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) void fetchNextPage({ cancelRefetch: false });
    }, { root: target.closest("main"), rootMargin: "0px 0px 160px" });
    observer.observe(target);
    return () => observer.disconnect();
  }, [fetchNextPage, hasNextPage, isFetching, isFetchNextPageError, chip]);

  return (
    <>
      <header className="content-header">
        <div className="feed-heading">
          {onBack && (
            <button ref={backButton} type="button" className="icon-button" aria-label="Back" title="Back" onClick={onBack}>
              <ChevronLeftIcon />
            </button>
          )}
          <h1>{title}</h1>
        </div>
        <button
          type="button"
          className="icon-button"
          aria-label="Refresh"
          title="Refresh"
          onClick={() => void feed.refetch()}
          disabled={isFetching}
        >
          <RefreshIcon />
        </button>
      </header>
      {source.kind === "home" && chips.length > 0 && (
        <div className="chips" role="group" aria-label="Home topics">
          <button type="button" className="chip" aria-pressed={chip === null} onClick={() => setChip(null)}>All</button>
          {chips.map(({ title: label, params }) => (
            <button type="button" key={params} className="chip" aria-pressed={chip === params} onClick={() => setChip(params)}>
              {label}
            </button>
          ))}
        </div>
      )}
      {feed.isPending && <p className="hint" role="status">Loading…</p>}
      {feed.isError && !isFetchNextPageError && (
        <div role="alert" className="screen-error">
          <p>{errorMessage(feed.error)}</p>
          <button type="button" onClick={() => void feed.refetch()} disabled={isFetching}>Retry</button>
        </div>
      )}
      {feed.data && sections.every((section) => section.items.length === 0) && <p className="hint">Nothing to show yet.</p>}
      {sections.map((section, index) => {
        if (section.items.length === 0) return null;
        const key = `${index}-${section.title}`;
        if (section.items.every((item) => item.kind === "category")) {
          return <CategoryGrid key={key} title={section.title} items={section.items} onOpen={onOpenCategory} />;
        }
        if (index === gridIndex) return <SongGrid key={key} section={section} onPlay={play} />;
        return <Carousel key={key} section={section} onPlay={play} onOpenPage={onOpenPage} onOpenCategory={onOpenCategory} />;
      })}
      {hasNextPage && (
        <div ref={sentinel} className="feed-more">
          {isFetchNextPageError && <p role="alert">{errorMessage(feed.error)}</p>}
          {feed.isFetchingNextPage && <p role="status" className="hint">Loading more…</p>}
          <button type="button" className="ghost" disabled={isFetching} onClick={() => void fetchNextPage({ cancelRefetch: false })}>
            {isFetchNextPageError ? "Retry loading more" : "Load more"}
          </button>
        </div>
      )}
    </>
  );
}

function CategoryGrid({ title, items, onOpen }: { title: string; items: CategoryItem[]; onOpen: Props["onOpenCategory"] }) {
  const headingId = useId();
  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header"><h2 id={headingId}>{title}</h2></div>
      <ul className="category-grid">
        {items.map((item, index) => (
          <li key={`${index}-${item.browse_id}`}><CategoryTile item={item} onOpen={onOpen} /></li>
        ))}
      </ul>
    </section>
  );
}

function CategoryTile({ item, onOpen }: { item: CategoryItem; onOpen: Props["onOpenCategory"] }) {
  return (
    <button type="button" className="category-tile" style={{ borderInlineStartColor: item.color ?? undefined }} onClick={() => onOpen(item)}>
      {item.title}
    </button>
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
  const items = section.items.filter(isPlayable);
  const songs = items.map(toSong);
  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header">
        <h2 id={headingId}>{section.title}</h2>
      </div>
      <ul className="song-grid">
        {items.map((item, index) => (
          <li key={`${index}-${itemKey(item)}`}>
            <ItemCard
              layout="row"
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onPlay={() => void onPlay(songs, index)}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** A shelf as a six-column carousel with previous/next buttons. */
function Carousel({ section, onPlay, onOpenPage, onOpenCategory }: ShelfProps & Pick<Props, "onOpenPage" | "onOpenCategory">) {
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
          <button type="button" className="icon-button" aria-label={`Previous in ${section.title}`} title={`Previous in ${section.title}`} onClick={() => page(-1)}>
            <ChevronLeftIcon size={16} />
          </button>
          <button type="button" className="icon-button" aria-label={`Next in ${section.title}`} title={`Next in ${section.title}`} onClick={() => page(1)}>
            <ChevronRightIcon size={16} />
          </button>
        </div>
      </div>
      <ul ref={list} className="carousel">
        {section.items.map((item, index) => (
          <li key={`${index}-${itemKey(item)}`}>
            {item.kind === "category" ? <CategoryTile item={item} onOpen={onOpenCategory} /> : (
              <ItemCard
                layout="card"
                shape={shapeOf(item)}
                title={item.title}
                subtitle={subtitleOf(item)}
                thumbnailUrl={item.thumbnail_url}
                onOpen={hasBrowsePage(item) ? () => onOpenPage(item) : undefined}
                onPlay={starts[index] >= 0 ? () => void onPlay(songs, starts[index]) : undefined}
              />
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
