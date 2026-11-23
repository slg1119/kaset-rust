import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { commands, type PlaylistActions as PlaylistStatus, type ShelfItem } from "../bindings";
import { errorMessage } from "../errors";
import { fetchLibraryCollection } from "../library/collection";
import { ActionDialog } from "../music/ActionDialog";
import { barePlaylistId, playlistQueries, useAccountMutation } from "../music/accountActions";
import { useNotice } from "../notices/NoticeProvider";
import { FEED_STALE_MS, queryKeys } from "../query";

export function PlaylistActions({ id, title, status, save, onDeleted, editing }: {
  id: string;
  title: string;
  status: PlaylistStatus;
  save: (saved: boolean) => void;
  onDeleted: () => void;
  editing: boolean;
}) {
  const { dismiss } = useNotice();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const libraryKey = queryKeys.library("playlists");
  const library = useQuery({
    queryKey: libraryKey,
    queryFn: () => fetchLibraryCollection("playlists"),
    enabled: !status.can_delete && status.saved == null,
    staleTime: FEED_STALE_MS,
  });
  const matches = (item: ShelfItem) =>
    item.kind === "playlist" && barePlaylistId(item.browse_id) === barePlaylistId(id);
  const saved = status.saved ?? library.data?.some(matches);
  const remove = useAccountMutation({
    request: () => commands.deletePlaylist(id),
    optimistic: { key: libraryKey, update: (items: ShelfItem[]) => items.filter((item) => !matches(item)) },
    invalidate: () => [libraryKey, queryKeys.playlistOptionsAll],
  });

  if (status.can_delete) {
    return (
      <>
        <button type="button" className="ghost" disabled={editing}
          onClick={() => { dismiss(); setConfirmDelete(true); }}>Delete playlist</button>
        {confirmDelete && (
          <ActionDialog title="Delete playlist?" busy={remove.isPending} onClose={() => setConfirmDelete(false)}>
            <p>“{title}” will be permanently deleted.</p>
            <div className="account-dialog-actions">
              <button type="button" disabled={remove.isPending} onClick={() => setConfirmDelete(false)}>Cancel</button>
              <button type="button" className="account-danger" disabled={remove.isPending}
                onClick={() => remove.mutate(undefined, {
                  onSuccess: () => { setConfirmDelete(false); onDeleted(); },
                })}>
                {remove.isPending ? "Deleting…" : "Delete playlist"}
              </button>
            </div>
          </ActionDialog>
        )}
      </>
    );
  }

  if (saved == null && library.isError) {
    return (
      <div role="alert" className="screen-error">
        <p>{errorMessage(library.error)}</p>
        <button type="button" disabled={library.isFetching} onClick={() => void library.refetch()}>Retry</button>
      </div>
    );
  }

  return (
    <button type="button" className="ghost" disabled={editing || saved == null}
      aria-pressed={saved} onClick={() => save(!saved)}>
      {saved ? "Remove from library" : "Save to library"}
    </button>
  );
}

export const playlistContentsQueries = (id: string) => [
  ...playlistQueries(id),
  queryKeys.playlistOptionsAll,
  queryKeys.library("playlists"),
];
