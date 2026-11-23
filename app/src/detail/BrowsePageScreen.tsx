import { useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { commands, type BrowsePage, type BrowsePageContinuation } from "../bindings";
import { errorMessage } from "../errors";
import { useLoadMore } from "../home/useLoadMore";
import { ChevronLeftIcon, PlayIcon, ShuffleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { artistNames } from "../music/items";
import { useNotice } from "../notices/NoticeProvider";
import { usePlayerQueue } from "../player/usePlayer";
import { MAX_QUEUE_LENGTH } from "../player/usePlayQueue";
import { BROWSE_PAGE_STALE_MS, queryKeys } from "../query";

type Props = {
  browseId: string;
  title: string;
  onBack?: () => void;
  active?: boolean;
  onOpenArtist?: OpenArtist;
};

type TrackPage = { header?: BrowsePage; tracks: BrowsePageContinuation };

const LIKED_SONGS_ID = "VLLM";

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

function playlistSongs(pages: TrackPage[]) {
  const cover = pages[0]?.header?.thumbnail_url;
  return pages.flatMap(({ tracks }) => tracks.songs.map((song) =>
    song.thumbnail_url === null && cover ? { ...song, thumbnail_url: cover } : song,
  ));
}

export function BrowsePageScreen({ browseId, title, onBack, onOpenArtist, active = true }: Props) {
  const page = useInfiniteQuery({
    queryKey: queryKeys.browsePage(browseId),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => fetchPage(browseId, pageParam),
    getNextPageParam: ({ tracks }, _pages, _pageParam, used) => {
      const token = tracks.continuation;
      return token && !used.includes(token) ? token : undefined;
    },
    staleTime: BROWSE_PAGE_STALE_MS,
  });
  const { report, dismiss } = useNotice();
  const queue = usePlayerQueue();
  const backButton = useRef<HTMLButtonElement>(null);
  const playback = useRef<AbortController | null>(null);
  const [starting, setStarting] = useState(false);
  const sentinel = useLoadMore(
    active && page.hasNextPage && !page.isFetching && !page.isFetchNextPageError,
    page.data?.pages.length ?? 0,
    page.fetchNextPage,
  );

  useEffect(() => {
    backButton.current?.focus();
  }, []);

  useLayoutEffect(() => () => playback.current?.abort(), [active, browseId]);

  const data = page.data?.pages[0]?.header;
  const headingId = useId();
  const pageTitle = data?.title || title;
  const songs = page.data ? playlistSongs(page.data.pages) : [];
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
        allSongs = playlistSongs(next.data.pages);
      }
      if (allSongs.length === 0) return;
      // A row beyond the IPC cap starts its own queue at that row.
      const offset = index < MAX_QUEUE_LENGTH ? 0 : index;
      const queued = allSongs.slice(offset, offset + MAX_QUEUE_LENGTH);
      const start = shuffle ? Math.floor(Math.random() * queued.length) : index - offset;
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
      {page.isPending && (
        <div className="browse-hero" role="status">
          <div className="browse-skeleton" aria-hidden="true" />
          <span className="hint">Loading…</span>
        </div>
      )}
      {page.isError && !page.isFetchNextPageError && (
        <div className="screen-error" role="alert">
          <p>{errorMessage(page.error)}</p>
          <button type="button" onClick={() => void page.refetch()} disabled={page.isFetching}>Retry</button>
        </div>
      )}
      {data && (
        <>
          <section className="browse-hero" aria-labelledby={headingId}>
            <Artwork url={data.thumbnail_url} size={200} />
            <div className="browse-summary">
              <h1 id={headingId}>{pageTitle}</h1>
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
              </div>
            </div>
          </section>
          {songs.length === 0 && !page.hasNextPage ? (
            <p className="hint">No tracks to play.</p>
          ) : (
            <ol className="rows" aria-label="Tracks">
              {songs.map((song, index) => (
                <li key={`${index}-${song.video_id}`}>
                  <div className="item item-row browse-track item-with-links" data-current={index === current || undefined}>
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
                  </div>
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
