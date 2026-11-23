import { useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef } from "react";
import { commands, type SearchItem } from "../bindings";
import { errorMessage } from "../errors";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { Artwork } from "../music/Artwork";
import { ItemCard, itemActionLabel } from "../music/ItemCard";
import { hasBrowsePage, isPlayable, itemKey, shapeOf, subtitleOf, toSong, type PageCard } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { queryKeys, SEARCH_STALE_MS } from "../query";
import { FILTERS, groupResults, KIND_LABEL, mergeSearchPages, type Filter, type ResultGroup } from "./groups";
import { SearchBox } from "./SearchBox";

type Props = {
  /** The submitted query; empty before the first search. */
  query: string;
  onSubmit: (query: string) => void;
  focusRequest: number;
  filter: Filter;
  onFilterChange: (filter: Filter) => void;
  onOpenPage: (card: PageCard) => void;
  onOpenArtist?: OpenArtist;
};

export function SearchScreen({ query, onSubmit, focusRequest, filter, onFilterChange, onOpenPage, onOpenArtist }: Props) {
  const serverFilter = filter === "all" ? null : filter;
  const results = useInfiniteQuery({
    queryKey: queryKeys.search(query, serverFilter),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => pageParam === null
      ? commands.search(query, serverFilter)
      : commands.searchContinuation(pageParam, serverFilter),
    getNextPageParam: ({ continuation }, _pages, _pageParam, used) =>
      continuation && !used.includes(continuation) ? continuation : undefined,
    enabled: query !== "",
    staleTime: SEARCH_STALE_MS,
  });
  const { fetchNextPage, hasNextPage, isFetching, isFetchNextPageError } = results;
  const sentinel = useRef<HTMLDivElement>(null);
  const play = usePlayQueue();
  const items = mergeSearchPages(results.data?.pages ?? []);
  const filterLabel = FILTERS.find((chip) => chip.value === filter)?.label ?? "Results";

  useEffect(() => {
    const target = sentinel.current;
    if (!target || !hasNextPage || isFetching || isFetchNextPageError || typeof IntersectionObserver === "undefined") return;
    let active = true;
    const observer = new IntersectionObserver((entries) => {
      if (active && entries.some((entry) => entry.isIntersecting)) {
        void fetchNextPage({ cancelRefetch: false });
      }
    }, { root: target.closest("main"), rootMargin: "0px 0px 160px" });
    observer.observe(target);
    return () => {
      active = false;
      observer.disconnect();
    };
  }, [fetchNextPage, hasNextPage, isFetching, isFetchNextPageError, query, filter]);

  function submit(next: string) {
    onFilterChange("all");
    onSubmit(next);
  }

  return (
    <>
      <header className="content-header">
        <SearchBox initialText={query} onSubmit={submit} focusRequest={focusRequest} />
      </header>
      <h1 className="visually-hidden">Search</h1>
      {query === "" && <p className="hint">Search for songs, videos, albums, artists, and playlists.</p>}
      {query !== "" && (
        <div className="chips" role="group" aria-label="Filter results">
          {FILTERS.map((chip) => (
            <button
              key={chip.value}
              type="button"
              className="chip"
              aria-pressed={filter === chip.value}
              onClick={() => onFilterChange(chip.value)}
            >
              {chip.label}
            </button>
          ))}
        </div>
      )}
      {results.isLoading && <p className="hint" role="status">Searching…</p>}
      {results.isError && !isFetchNextPageError && (
        <div role="alert" className="screen-error">
          <p>{errorMessage(results.error)}</p>
          <button type="button" onClick={() => void results.refetch()} disabled={isFetching}>
            Retry
          </button>
        </div>
      )}
      {results.data && !hasNextPage && items.length === 0 && <p className="hint">No results for “{query}”.</p>}
      {items.length > 0 && (filter === "all" ? (
        <AllResults items={items} onPlay={play} onOpenPage={onOpenPage} onOpenArtist={onOpenArtist} />
      ) : (
        <Group group={{ title: filterLabel, items }} onPlay={play} onOpenPage={onOpenPage} onOpenArtist={onOpenArtist} />
      ))}
      {hasNextPage && (
        <div ref={sentinel} className="feed-more">
          {isFetchNextPageError && <p role="alert">{errorMessage(results.error)}</p>}
          {results.isFetchingNextPage && <p role="status" className="hint">Loading more…</p>}
          <button type="button" className="ghost" disabled={isFetching} onClick={() => void fetchNextPage({ cancelRefetch: false })}>
            {isFetchNextPageError ? "Retry loading more" : "Load more"}
          </button>
        </div>
      )}
    </>
  );
}

type ResultActions = Pick<Props, "onOpenPage" | "onOpenArtist"> & { onPlay: ReturnType<typeof usePlayQueue> };

/** The top result as a large card, then every group without it. */
function AllResults({ items, onPlay, onOpenPage, onOpenArtist }: { items: SearchItem[] } & ResultActions) {
  const [top, ...rest] = items;
  return (
    <>
      <TopResult item={top} onPlay={onPlay} onOpenPage={onOpenPage} onOpenArtist={onOpenArtist} />
      {groupResults(rest).map((group) => (
        <Group key={group.title} group={group} onPlay={onPlay} onOpenPage={onOpenPage} onOpenArtist={onOpenArtist} />
      ))}
    </>
  );
}

function TopResult({ item, onPlay, onOpenPage, onOpenArtist }: { item: SearchItem } & ResultActions) {
  const headingId = useId();
  const subtitle = subtitleOf(item);
  const artwork = <Artwork url={item.thumbnail_url} size={96} shape={shapeOf(item)} />;
  const title = <span className="top-title">{item.title}</span>;
  const metadata = (
    <span className="item-subtitle">
      {KIND_LABEL[item.kind]}
      {subtitle && <> • {isPlayable(item) ? <ArtistNames artists={item.artists} onOpenArtist={onOpenArtist} /> : subtitle}</>}
    </span>
  );
  return (
    <section className="top-result" aria-labelledby={headingId}>
      <h2 id={headingId}>Top result</h2>
      {hasBrowsePage(item) ? (
        <button type="button" className="top-card" aria-label={itemActionLabel("Open", item.title, subtitle)} onClick={() => onOpenPage(item)}>
          {artwork}
          <span className="top-text">{title}{metadata}</span>
        </button>
      ) : (
        <div className="top-card">
          {artwork}
          <div className="top-text">
            {title}
            {metadata}
            {isPlayable(item) && (
              <button type="button" className="primary" onClick={() => void onPlay([toSong(item)], 0)}>
                Play
              </button>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

const MAX_QUEUE_LENGTH = 500;

function Group({ group, onPlay, onOpenPage, onOpenArtist }: { group: ResultGroup } & ResultActions) {
  const headingId = useId();
  const songs = group.items.filter(isPlayable).map(toSong);
  let nextSongIndex = 0;

  function playFrom(index: number) {
    const offset = index < MAX_QUEUE_LENGTH ? 0 : index;
    return onPlay(songs.slice(offset, offset + MAX_QUEUE_LENGTH), index - offset);
  }
  return (
    <section className="result-group" aria-labelledby={headingId}>
      <h2 id={headingId}>{group.title}</h2>
      <ul className="rows">
        {group.items.map((item) => {
          const songIndex = isPlayable(item) ? nextSongIndex++ : -1;
          return (
            <li key={itemKey(item)}>
              <ItemCard
                layout="row"
                shape={shapeOf(item)}
                title={item.title}
                subtitle={subtitleOf(item)}
                thumbnailUrl={item.thumbnail_url}
                artistCredits={isPlayable(item) && onOpenArtist && item.artists.some(({ browse_id }) => browse_id) ?
                  <ArtistNames artists={item.artists} onOpenArtist={onOpenArtist} /> : undefined}
                onOpen={hasBrowsePage(item) ? () => onOpenPage(item) : undefined}
                onPlay={songIndex >= 0 ? () => void playFrom(songIndex) : undefined}
              />
            </li>
          );
        })}
      </ul>
    </section>
  );
}
