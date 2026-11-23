import { useInfiniteQuery, useQueryClient, type InfiniteData } from "@tanstack/react-query";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { commands, type BrowsePage, type BrowsePageContinuation } from "../bindings";
import { errorMessage } from "../errors";
import { useLoadMore } from "../home/useLoadMore";
import { ChevronLeftIcon, PlayIcon, ShuffleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { artistNames } from "../music/items";
import { playlistQueries, useAccountMutation } from "../music/accountActions";
import { SongMenu } from "../music/SongMenu";
import { useNotice } from "../notices/NoticeProvider";
import { usePlayerQueue } from "../player/usePlayer";
import { MAX_QUEUE_LENGTH } from "../player/usePlayQueue";
import { BROWSE_PAGE_STALE_MS, queryKeys } from "../query";

import { PlaylistActions, playlistContentsQueries } from "./PlaylistActions";

type Props = {
  browseId: string;
  title: string;
  onBack?: () => void;
  active?: boolean;
  onOpenArtist?: OpenArtist;
  isAlbum?: boolean;
};

type TrackPage = { header?: BrowsePage; tracks: BrowsePageContinuation };
type TrackPages = InfiniteData<TrackPage, string | null>;

function removePlaylistTrack(data: TrackPages, setVideoId: string): TrackPages {
  return {
    ...data,
    pages: data.pages.map((entry) => {
      const ids = entry.tracks.set_video_ids;
      if (!ids?.includes(setVideoId)) return entry;
      return {
        ...entry,
        tracks: {
          ...entry.tracks,
          songs: entry.tracks.songs.filter((_, index) => ids[index] !== setVideoId),
          set_video_ids: ids.filter((id) => id !== setVideoId),
        },
      };
    }),
  };
}

const LIKED_SONGS_ID = "VLLM";

function randomStartIndex(length: number) {
  return Math.floor(Math.random() * length);
}

async function fetchPage(browseId: string, cursor: string | null): Promise<TrackPage> {
  if (cursor !== null) {
    return { tracks: await commands.getBrowsePageContinuation(cursor) };
  }
  if (browseId === LIKED_SONGS_ID) {
    const library = await commands.getLibrary("songs");
    if (library.kind !== "songs") throw new Error("Could not read liked songs.");
    return { header: library.data, tracks: library.data };
  }
  const header = await commands.getBrowsePage(browseId);
  return { header, tracks: header };
}

function playlistRows(pages: TrackPage[]) {
  const cover = pages[0]?.header?.thumbnail_url;
  return pages.flatMap(({ tracks }) => tracks.songs.map((song, index) => ({
    song: song.thumbnail_url === null && cover ? { ...song, thumbnail_url: cover } : song,
    setVideoId: tracks.set_video_ids?.[index],
  })));
}

export function BrowsePageScreen({ browseId, title, onBack, onOpenArtist, isAlbum, active = true }: Props) {
  const key = queryKeys.browsePage(browseId);
  const queryClient = useQueryClient();
  const [deleted, setDeleted] = useState(false);
  const page = useInfiniteQuery({
    queryKey: key,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => fetchPage(browseId, pageParam),
    getNextPageParam: ({ tracks }, _pages, _pageParam, used) => {
      const token = tracks.continuation;
      return token && !used.includes(token) ? token : undefined;
    },
    staleTime: BROWSE_PAGE_STALE_MS,
    enabled: !deleted,
  });
  const { report, dismiss } = useNotice();
  const save = useAccountMutation({
    request: (saved: boolean) => commands.setPlaylistSaved(browseId, saved),
    optimistic: {
      key,
      update: (data: TrackPages, saved) => ({
        ...data,
        pages: data.pages.map((entry) => entry.header?.playlist ? {
          ...entry,
          header: { ...entry.header, playlist: { ...entry.header.playlist, saved } },
        } : entry),
      }),
    },
    invalidate: () => [...playlistQueries(browseId), queryKeys.library("playlists")],
  });
  const removeTrack = useAccountMutation({
    request: ({ videoId, setVideoId }: { videoId: string; setVideoId: string }) =>
      commands.removeFromPlaylist(browseId, videoId, setVideoId),
    optimistic: { key, update: (data: TrackPages, { setVideoId }) => removePlaylistTrack(data, setVideoId) },
    invalidate: () => playlistContentsQueries(browseId),
  });
  const queue = usePlayerQueue();
  const backButton = useRef<HTMLButtonElement>(null);
  const playback = useRef<AbortController | null>(null);
  const [starting, setStarting] = useState(false);
  const sentinel = useLoadMore(
    active && page.hasNextPage && !page.isFetching && !page.isFetchNextPageError,
    page.data?.pages.length ?? 0,
    page.fetchNextPage,
  );
  const albumTitle = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    backButton.current?.focus();
  }, []);

  useLayoutEffect(() => () => playback.current?.abort(), [active, browseId]);

  const data = page.data?.pages[0]?.header;
  const headingId = useId();
  const pageTitle = data?.title || title;
  const rows = page.data ? playlistRows(page.data.pages) : [];
  const songs = rows.map(({ song }) => song);
  const playlist = !isAlbum && browseId !== LIKED_SONGS_ID ? data?.playlist : undefined;
  const editing = save.isPending || removeTrack.isPending;

  function leaveDeletedPlaylist() {
    setDeleted(true);
    for (const queryKey of playlistQueries(browseId)) {
      void queryClient.invalidateQueries({ queryKey, refetchType: "none" });
    }
    onBack?.();
  }
  const track = queue?.browse_track;
  const current = track?.browse_id === browseId && queue?.current != null
    && songs[track.index]?.video_id === queue.songs[queue.current]?.video_id
    ? track.index
    : -1;

  async function playFrom(index: number, loadRemaining = false, shuffle = false) {
    if (playback.current) return;
    const controller = new AbortController();
    playback.current = controller;
    setStarting(true);
    dismiss();
    try {
      let allSongs = songs;
      let hasMore = page.hasNextPage;
      while (loadRemaining && hasMore && allSongs.length < MAX_QUEUE_LENGTH) {
        const next = await page.fetchNextPage({ cancelRefetch: false, throwOnError: true });
        if (controller.signal.aborted) return;
        if (!next.data) return;
        hasMore = next.hasNextPage;
        allSongs = playlistRows(next.data.pages).map(({ song }) => song);
      }
      if (allSongs.length === 0) return;
      // A row beyond the IPC cap starts its own queue at that row.
      const offset = index < MAX_QUEUE_LENGTH ? 0 : index;
      const queued = allSongs.slice(offset, offset + MAX_QUEUE_LENGTH);
      const start = shuffle ? randomStartIndex(queued.length) : index - offset;
      await commands.playBrowseQueue(queued, start, { browse_id: browseId, offset, shuffle });
    } catch (cause) {
      if (!controller.signal.aborted) report(errorMessage(cause));
    } finally {
      playback.current = null;
      setStarting(false);
    }
  }

  return (
    <>
      <header className="content-header browse-header">
        {onBack && (
          <button ref={backButton} type="button" className="icon-button" aria-label="Back" title="Back" onClick={onBack}>
            <ChevronLeftIcon />
          </button>
        )}
        <span>{pageTitle}</span>
      </header>
      {deleted && <p role="status">Playlist deleted.</p>}
      {!deleted && page.isPending && (
        <div className="browse-hero" role="status">
          <div className="browse-skeleton" aria-hidden="true" />
          <span className="hint">Loading…</span>
        </div>
      )}
      {!deleted && page.isError && !page.isFetchNextPageError && (
        <div className="screen-error" role="alert">
          <p>{errorMessage(page.error)}</p>
          <button type="button" onClick={() => void page.refetch()} disabled={page.isFetching}>Retry</button>
        </div>
      )}
      {!deleted && data && (
        <>
          <section className="browse-hero" aria-labelledby={headingId}>
            <Artwork url={data.thumbnail_url} size={200} />
            <div className="browse-summary">
              <h1 id={headingId} ref={albumTitle} tabIndex={isAlbum ? -1 : undefined}>{pageTitle}</h1>
              {data.subtitle && <p>{data.subtitle}</p>}
              {data.second_subtitle && <p>{data.second_subtitle}</p>}
              <div className="browse-actions">
                <button
                  type="button"
                  className="primary browse-play"
                  aria-label="Play"
                  title="Play"
                  onClick={() => void playFrom(0, true)}
                  disabled={starting || (songs.length === 0 && !page.hasNextPage)}
                >
                  <PlayIcon />
                </button>
                <button type="button" className="ghost browse-shuffle" onClick={() => void playFrom(0, true, true)} disabled={starting || (songs.length === 0 && !page.hasNextPage)}>
                  <ShuffleIcon />
                  Shuffle
                </button>
                {playlist && <PlaylistActions id={browseId} title={pageTitle} status={playlist}
                  editing={editing} save={(saved) => save.mutate(saved)} onDeleted={leaveDeletedPlaylist} />}
              </div>
            </div>
          </section>
          {songs.length === 0 && !page.hasNextPage ? (
            <p className="hint">No tracks to play.</p>
          ) : (
            <ol className="rows" aria-label="Tracks">
              {rows.map(({ song, setVideoId }, index) => (
                <li key={setVideoId ?? `${index}-${song.video_id}`}>
                  <SongMenu
                    className={`item item-row browse-track item-with-links${index === current ? " is-current" : ""}`}
                    song={song}
                    onOpenArtist={onOpenArtist}
                    onRemoveFromPlaylist={playlist?.can_delete && setVideoId && !editing
                      ? () => removeTrack.mutate({ videoId: song.video_id, setVideoId })
                      : undefined}
                    onOpenAlbum={isAlbum ? () => albumTitle.current?.focus() : undefined}
                  >
                    <button
                      type="button"
                      className="item-action"
                      aria-label={[song.title, artistNames(song.artists)].filter(Boolean).join(", ")}
                      aria-current={index === current ? "true" : undefined}
                      aria-disabled={starting || undefined}
                      onClick={() => void playFrom(index)}
                    />
                    <span className="browse-index" aria-hidden="true">{index + 1}</span>
                    <span className="item-text">
                      <span className="item-title">{song.title}</span>
                      {song.artists.length > 0 && (
                        <span className="item-subtitle"><ArtistNames artists={song.artists} onOpenArtist={onOpenArtist} /></span>
                      )}
                    </span>
                  </SongMenu>
                </li>
              ))}
            </ol>
          )}
          {page.hasNextPage && (
            <div ref={sentinel} className="feed-more">
              {page.isFetchNextPageError && <p role="alert">{errorMessage(page.error)}</p>}
              {page.isFetchingNextPage && <p role="status" className="hint">Loading more…</p>}
              <button type="button" className="ghost" disabled={page.isFetching || starting}
                onClick={() => void page.fetchNextPage({ cancelRefetch: false })}>
                {page.isFetchNextPageError ? "Retry loading more" : "Load more"}
              </button>
            </div>
          )}
        </>
      )}
    </>
  );
}
