import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { commands, type SearchItem } from "../bindings";
import { errorMessage } from "../errors";
import { Artwork } from "../music/Artwork";
import { ItemCard } from "../music/ItemCard";
import { hasBrowsePage, isPlayable, itemKey, shapeOf, subtitleOf, toSong, type OpenableItem } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { queryKeys, SEARCH_STALE_MS } from "../query";
import { FILTERS, groupResults, KIND_LABEL, type Filter, type ResultGroup } from "./groups";
import { SearchBox } from "./SearchBox";

type Props = {
  /** The submitted query; empty before the first search. */
  query: string;
  onSubmit: (query: string) => void;
  focusRequest: number;
  filter: Filter;
  onFilterChange: (filter: Filter) => void;
  onOpenPage: (card: OpenableItem) => void;
};

export function SearchScreen({ query, onSubmit, focusRequest, filter, onFilterChange, onOpenPage }: Props) {
  const results = useQuery({
    queryKey: queryKeys.search(query),
    queryFn: () => commands.search(query),
    enabled: query !== "",
    staleTime: SEARCH_STALE_MS,
  });
  const play = usePlayQueue();
  const items = results.data?.items ?? [];

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
      {results.isLoading && <p className="hint" role="status">Searching…</p>}
      {results.isError && (
        <div role="alert" className="screen-error">
          <p>{errorMessage(results.error)}</p>
          <button type="button" onClick={() => void results.refetch()}>
            Retry
          </button>
        </div>
      )}
      {results.data && items.length === 0 && <p className="hint">No results for “{query}”.</p>}
      {items.length > 0 && (
        <>
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
          {filter === "all" ? (
            <AllResults items={items} onPlay={play} onOpenPage={onOpenPage} />
          ) : (
            <FilteredResults items={items} filter={filter} onPlay={play} onOpenPage={onOpenPage} />
          )}
        </>
      )}
    </>
  );
}

type ResultActions = Pick<Props, "onOpenPage"> & { onPlay: ReturnType<typeof usePlayQueue> };

/** The top result as a large card, then every group without it. */
function AllResults({ items, onPlay, onOpenPage }: { items: SearchItem[] } & ResultActions) {
  const [top, ...rest] = items;
  return (
    <>
      <TopResult item={top} onPlay={onPlay} onOpenPage={onOpenPage} />
      {groupResults(rest).map((group) => (
        <Group key={group.title} group={group} onPlay={onPlay} onOpenPage={onOpenPage} />
      ))}
    </>
  );
}

function FilteredResults({ items, filter, onPlay, onOpenPage }: { items: SearchItem[]; filter: Filter } & ResultActions) {
  const title = FILTERS.find((chip) => chip.value === filter)?.group;
  const group = groupResults(items).find((candidate) => candidate.title === title);
  if (group === undefined) return <p className="hint">No {title?.toLowerCase()} in these results.</p>;
  return <Group group={group} onPlay={onPlay} onOpenPage={onOpenPage} />;
}

function TopResult({ item, onPlay, onOpenPage }: { item: SearchItem } & ResultActions) {
  const headingId = useId();
  const subtitle = subtitleOf(item);
  const artwork = <Artwork url={item.thumbnail_url} size={96} shape={shapeOf(item)} />;
  const title = <span className="top-title">{item.title}</span>;
  const metadata = <span className="item-subtitle">{[KIND_LABEL[item.kind], subtitle].filter(Boolean).join(" • ")}</span>;
  return (
    <section className="top-result" aria-labelledby={headingId}>
      <h2 id={headingId}>Top result</h2>
      {hasBrowsePage(item) ? (
        <button type="button" className="top-card" aria-label={`Open ${item.title}`} onClick={() => onOpenPage(item)}>
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

function Group({ group, onPlay, onOpenPage }: { group: ResultGroup } & ResultActions) {
  const headingId = useId();
  const songs = group.playable ? group.items.filter(isPlayable).map(toSong) : [];
  return (
    <section className="result-group" aria-labelledby={headingId}>
      <h2 id={headingId}>{group.title}</h2>
      <ul className="rows">
        {group.items.map((item, index) => (
          <li key={itemKey(item)}>
            <ItemCard
              layout="row"
              shape={shapeOf(item)}
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onOpen={hasBrowsePage(item) ? () => onOpenPage(item) : undefined}
              onPlay={group.playable && isPlayable(item) ? () => void onPlay(songs, index) : undefined}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}
