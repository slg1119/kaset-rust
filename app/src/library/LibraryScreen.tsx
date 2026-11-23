import { useQuery } from "@tanstack/react-query";
import type { LibraryKind } from "../bindings";
import { errorMessage } from "../errors";
import { FeedShelf, type CategoryItem } from "../home/FeedShelf";
import { RefreshIcon } from "../icons";
import type { PageCard } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { FEED_STALE_MS, queryKeys } from "../query";

import { fetchLibraryCollection } from "./collection";

type Props = {
  collection: Exclude<LibraryKind, "songs">;
  title: string;
  onOpenPage: (card: PageCard) => void;
  onOpenCategory: (item: CategoryItem) => void;
};

export function LibraryScreen({ collection, title, onOpenPage, onOpenCategory }: Props) {
  const query = useQuery({
    queryKey: queryKeys.library(collection),
    queryFn: () => fetchLibraryCollection(collection),
    staleTime: FEED_STALE_MS,
  });
  const play = usePlayQueue();
  const refresh = () => void query.refetch();
  return (
    <>
      <header className="content-header">
        <h1>{title}</h1>
        <button type="button" className="icon-button" aria-label="Refresh" title="Refresh" disabled={query.isFetching} onClick={refresh}>
          <RefreshIcon />
        </button>
      </header>
      {query.isPending && <p className="hint" role="status">Loading…</p>}
      {query.isError && (
        <div role="alert" className="screen-error">
          <p>{errorMessage(query.error)}</p>
          <button type="button" disabled={query.isFetching} onClick={refresh}>Retry</button>
        </div>
      )}
      {query.data?.length === 0 && <p className="hint">Nothing saved yet.</p>}
      {query.data && query.data.length > 0 && (
        <FeedShelf
          section={{ title, items: query.data }}
          layout="grid"
          onPlay={play}
          onOpenPage={onOpenPage}
          onOpenCategory={onOpenCategory}
        />
      )}
    </>
  );
}
