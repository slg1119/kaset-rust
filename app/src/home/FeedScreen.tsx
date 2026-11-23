import { skipToken, useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { commands, type FeedKind } from "../bindings";
import { errorMessage } from "../errors";
import { ChevronLeftIcon, RefreshIcon } from "../icons";
import type { OpenArtist } from "../music/ArtistNames";
import { isPlayable, type PageCard } from "../music/items";
import { CategoryGrid, FeedShelf, type CategoryItem } from "./FeedShelf";

export type { CategoryItem } from "./FeedShelf";
import { usePlayQueue } from "../player/usePlayQueue";
import { FEED_STALE_MS, nextPageParam, queryKeys } from "../query";

export type FeedSource = { kind: FeedKind | "podcasts" } | { kind: "category"; browseId: string; params: string | null };

type Props = {
  source: FeedSource;
  title: string;
  onOpenPage: (card: PageCard) => void;
  onOpenArtist?: OpenArtist;
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

export function FeedScreen({ source, title, onOpenPage, onOpenCategory, onOpenArtist, onBack }: Props) {
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
        return (
          <FeedShelf key={key} section={section} onPlay={play} onOpenPage={onOpenPage} onOpenCategory={onOpenCategory}
            onOpenArtist={onOpenArtist} layout={index === gridIndex ? "quick-picks" : "carousel"} />
        );
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
