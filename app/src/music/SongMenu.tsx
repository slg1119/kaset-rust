import { useIsMutating, useQueryClient } from "@tanstack/react-query";
import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { commands, type LibraryAction, type Song, type SongActionStatus } from "../bindings";
import { errorMessage } from "../errors";
import { MoreIcon } from "../icons";
import { useNotice } from "../notices/NoticeProvider";
import { queryKeys } from "../query";
import { AddToPlaylistDialog } from "./AddToPlaylistDialog";
import { songLibraryQueries, songMutationKey, useAccountMutation, useSongMetadata } from "./accountActions";
import type { OpenArtist } from "./ArtistNames";

type Props = {
  song: Song;
  children: ReactNode;
  className?: string;
  onOpenArtist?: OpenArtist;
  onOpenAlbum?: () => void;
  onRemoveFromPlaylist?: () => void;
};

export function SongMenu({ song, children, className, onOpenArtist, onOpenAlbum, onRemoveFromPlaylist }: Props) {
  const { dismiss, report } = useNotice();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [anchor, setAnchor] = useState<{ x: number; y: number; last?: boolean } | null>(null);
  const [playlistOpen, setPlaylistOpen] = useState(false);
  const metadata = useSongMetadata(song.video_id, !!anchor);
  const key = queryKeys.songMetadata(song.video_id);
  const mutationKey = songMutationKey(song.video_id);
  const editingLibrary = useIsMutating({ mutationKey, exact: true }) > 0;
  const client = useQueryClient();
  const edit = useAccountMutation({
    mutationKey,
    request: (action: LibraryAction) => commands.editLibrary({ video_id: song.video_id, action }),
    optimistic: {
      key,
      update: (data: SongActionStatus, action) => ({ ...data, in_library: action === "add" }),
    },
    invalidate: () => [key, ...songLibraryQueries],
  });
  const libraryAction: LibraryAction = metadata.data?.in_library ? "remove" : "add";
  const canEdit = libraryAction === "add" ? metadata.data?.can_add : metadata.data?.can_remove;
  const label = `More actions for ${song.title}`;
  const artist = song.artists.find(({ browse_id }) => browse_id);

  function close(restoreFocus: boolean) {
    setAnchor(null);
    if (restoreFocus) trigger.current?.focus();
  }

  function openFromButton(last = false) {
    const rect = trigger.current?.getBoundingClientRect();
    if (rect) setAnchor({ x: rect.right, y: rect.bottom + 4, last });
  }

  useLayoutEffect(() => {
    const popup = menu.current;
    if (!anchor || !popup) return;
    const margin = 8;
    popup.style.left = `${Math.max(margin, Math.min(anchor.x, window.innerWidth - popup.offsetWidth - margin))}px`;
    popup.style.top = `${Math.max(margin, Math.min(anchor.y, window.innerHeight - popup.offsetHeight - margin))}px`;
    const items = popup.querySelectorAll<HTMLButtonElement>("button:not(:disabled)");
    items[anchor.last ? items.length - 1 : 0]?.focus({ preventScroll: true });

    function onPointerDown(event: PointerEvent) {
      if (event.target instanceof Node && !popup?.contains(event.target) && !trigger.current?.contains(event.target)) {
        setAnchor(null);
      }
    }
    function onScroll(event: Event) {
      if (event.target instanceof Node && popup?.contains(event.target)) return;
      setAnchor(null);
    }
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [anchor]);

  function handleMenuKey(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Tab") {
      close(true);
      return;
    }
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    let next: number;
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        close(true);
        return;
      case "ArrowDown": next = (index + 1) % items.length; break;
      case "ArrowUp": next = (index + items.length - 1) % items.length; break;
      case "Home": next = 0; break;
      case "End": next = items.length - 1; break;
      default: return;
    }
    event.preventDefault();
    items[next]?.focus();
  }

  function run(action: () => Promise<boolean | null>) {
    close(true);
    dismiss();
    action()
      .then((changed) => {
        if (changed === false) report("Already in the queue");
      })
      .catch((cause: unknown) => report(errorMessage(cause)));
  }

  return (
    <div
      className={className ? `${className} song-menu-host` : "song-menu-host"}
      onContextMenu={(event) => {
        if (event.target instanceof Element && event.target.closest("dialog")) return;
        event.preventDefault();
        setAnchor({ x: event.clientX, y: event.clientY });
      }}
    >
      {children}
      <button
        ref={trigger}
        type="button"
        className="icon-button song-menu-trigger"
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        onClick={() => anchor ? close(true) : openFromButton()}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
          event.preventDefault();
          openFromButton(event.key === "ArrowUp");
        }}
      >
        <MoreIcon size={16} />
      </button>
      {anchor && createPortal(
        <div ref={menu} className="song-menu" role="menu" aria-label={label} onKeyDown={handleMenuKey}>
          <button type="button" role="menuitem" tabIndex={-1} onClick={() => run(() => commands.insertNext([song]))}>Play next</button>
          <button type="button" role="menuitem" tabIndex={-1} onClick={() => run(() => commands.enqueue([song]))}>Add to queue</button>
          <button type="button" role="menuitem" tabIndex={-1} onClick={() => run(() => commands.startRadio(song))}>Start radio</button>
          {onOpenArtist && artist?.browse_id && (
            <button type="button" role="menuitem" tabIndex={-1} onClick={() => {
              close(true);
              if (artist.browse_id) onOpenArtist(artist.browse_id, artist.name);
            }}>Go to artist</button>
          )}
          {onOpenAlbum && (
            <button type="button" role="menuitem" tabIndex={-1} onClick={() => {
              close(true);
              onOpenAlbum();
            }}>Go to album</button>
          )}
          <button type="button" role="menuitem" tabIndex={-1}
            disabled={editingLibrary || metadata.isFetching || !canEdit}
            onClick={() => {
              close(true);
              edit.mutate(libraryAction, { onError: () => { void client.invalidateQueries({ queryKey: key }); } });
            }}>
            {metadata.data?.in_library ? "Remove from library" : "Add to library"}
          </button>
          {metadata.isError && (
            <button type="button" role="menuitem" tabIndex={-1} onClick={() => { void metadata.refetch(); }}>
              Retry loading library status
            </button>
          )}
          <button type="button" role="menuitem" tabIndex={-1}
            onClick={() => { close(true); dismiss(); setPlaylistOpen(true); }}>
            Add to playlist…
          </button>
          {onRemoveFromPlaylist && (
            <button type="button" role="menuitem" tabIndex={-1} onClick={() => { close(true); onRemoveFromPlaylist(); }}>
              Remove from this playlist
            </button>
          )}
        </div>,
        document.body,
      )}
      {playlistOpen && <AddToPlaylistDialog song={song} onClose={() => setPlaylistOpen(false)} />}
    </div>
  );
}
