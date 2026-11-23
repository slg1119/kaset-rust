import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { commands, type AddToPlaylistMenu, type PlaylistPrivacy, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { queryKeys } from "../query";
import { ActionDialog } from "./ActionDialog";
import { Artwork } from "./Artwork";
import { playlistQueries, useAccountMutation } from "./accountActions";

export function AddToPlaylistDialog({ song, onClose }: { song: Song; onClose: () => void }) {
  const key = queryKeys.playlistOptions(song.video_id);
  const options = useQuery({
    queryKey: key,
    queryFn: () => commands.addToPlaylistOptions(song.video_id),
  });
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [privacy, setPrivacy] = useState<PlaylistPrivacy>("PRIVATE");
  const add = useAccountMutation({
    request: (id: string) => commands.addToPlaylist(song.video_id, id),
    optimistic: {
      key,
      update: (menu: AddToPlaylistMenu, id) => ({
        ...menu,
        options: menu.options.map((option) =>
          option.playlist_id === id ? { ...option, is_selected: true } : option),
      }),
    },
    invalidate: (id) => [key, ...playlistQueries(id), queryKeys.library("playlists")],
  });
  const create = useAccountMutation({
    request: () => commands.createPlaylist(title.trim(), description, privacy, [song.video_id]),
    invalidate: () => [queryKeys.playlistOptionsAll, queryKeys.library("playlists")],
  });
  const busy = add.isPending || create.isPending;
  const titleTooLong = Array.from(title).length > 150;
  const descriptionTooLong = Array.from(description).length > 5000;
  const canCreate = !busy && !!title.trim() && !titleTooLong && !descriptionTooLong;

  return (
    <ActionDialog title={creating ? "New playlist" : options.data?.title || "Add to playlist"} onClose={onClose} busy={busy}>
      {creating ? (
        <form className="account-dialog-form" onSubmit={(event) => {
          event.preventDefault();
          if (canCreate) create.mutate(undefined, { onSuccess: onClose });
        }}>
          <label>
            Title
            <input required autoFocus aria-invalid={titleTooLong} value={title} onChange={(event) => setTitle(event.target.value)} />
            {titleTooLong && <span className="hint">Use at most 150 characters.</span>}
          </label>
          <label>
            Description
            <textarea aria-invalid={descriptionTooLong} value={description} onChange={(event) => setDescription(event.target.value)} />
            {descriptionTooLong && <span className="hint">Use at most 5,000 characters.</span>}
          </label>
          <label>Privacy<select value={privacy} onChange={(event) => {
            const value = event.target.value;
            if (value === "PRIVATE" || value === "PUBLIC" || value === "UNLISTED") setPrivacy(value);
          }}>
            <option value="PRIVATE">Private</option>
            <option value="UNLISTED">Unlisted</option>
            <option value="PUBLIC">Public</option>
          </select></label>
          <div className="account-dialog-actions">
            <button type="button" disabled={busy} onClick={() => setCreating(false)}>Back</button>
            <button type="submit" className="primary" disabled={!canCreate}>
              {create.isPending ? "Creating…" : "Create playlist"}
            </button>
          </div>
        </form>
      ) : (
        <>
          {options.isPending && <p role="status">Loading playlists…</p>}
          {options.isError && (
            <div className="screen-error" role="alert">
              <p>{errorMessage(options.error)}</p>
              <button type="button" disabled={options.isFetching} onClick={() => void options.refetch()}>Retry</button>
            </div>
          )}
          {options.data?.options.length === 0 && <p className="hint">No playlists available.</p>}
          {!!options.data?.options.length && (
            <ul className="account-playlist-options">
              {options.data.options.map((option) => (
                <li key={option.playlist_id}>
                  <button type="button" disabled={busy || option.is_selected}
                    aria-label={option.is_selected ? `${option.title}, added` : option.title}
                    onClick={() => add.mutate(option.playlist_id, { onSuccess: onClose })}>
                    <Artwork url={option.thumbnail_url} size={32} />
                    <span>{option.title}</span>
                    {option.is_selected && <span className="hint">Added</span>}
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="account-dialog-actions">
            <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
            {options.data?.can_create_playlist && (
              <button type="button" className="primary" onClick={() => setCreating(true)} disabled={busy}>New playlist</button>
            )}
          </div>
        </>
      )}
    </ActionDialog>
  );
}
