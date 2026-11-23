import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { ChevronLeftIcon, PlayIcon, ShuffleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { artistNames } from "../music/items";
import { SongMenu } from "../music/SongMenu";
import { useNotice } from "../notices/NoticeProvider";
import { usePlayer } from "../player/usePlayer";
import { MAX_QUEUE_LENGTH, usePlayQueue } from "../player/usePlayQueue";
import { BROWSE_PAGE_STALE_MS, queryKeys } from "../query";

type Props = {
  browseId: string;
  title: string;
  onBack: () => void;
  onOpenArtist?: OpenArtist;
  isAlbum?: boolean;
};

export function BrowsePageScreen({ browseId, title, onBack, onOpenArtist, isAlbum }: Props) {
  const page = useQuery({
    queryKey: queryKeys.browsePage(browseId),
    queryFn: () => commands.getBrowsePage(browseId),
    staleTime: BROWSE_PAGE_STALE_MS,
  });
  const { report, dismiss } = useNotice();
  const { state } = usePlayer(report);
  const play = usePlayQueue();
  const backButton = useRef<HTMLButtonElement>(null);
  const albumTitle = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    backButton.current?.focus();
  }, []);

  const data = page.data;
  const songs = data?.songs ?? [];
  const current = songs.findIndex((song) => song.video_id === state?.track?.video_id);

  function playFrom(index: number) {
    // Keep rows beyond the IPC limit playable by queuing from that row onward.
    const offset = index < MAX_QUEUE_LENGTH ? 0 : index;
    return play(songs.slice(offset, offset + MAX_QUEUE_LENGTH), index - offset);
  }

  async function shuffle() {
    dismiss();
    try {
      await commands.setShuffle(true);
    } catch (cause) {
      report(errorMessage(cause));
      return;
    }
    // The player shuffles after the requested starting song.
    await playFrom(Math.floor(Math.random() * Math.min(songs.length, MAX_QUEUE_LENGTH)));
  }

  return (
    <>
      <header className="content-header browse-header">
        <button ref={backButton} type="button" className="icon-button" aria-label="Back" onClick={onBack}>
          <ChevronLeftIcon />
        </button>
        <span>{data?.title ?? title}</span>
      </header>
      {page.isPending && (
        <div className="browse-hero" role="status">
          <div className="browse-skeleton" aria-hidden="true" />
          <span className="hint">Loading…</span>
        </div>
      )}
      {page.isError && (
        <div className="screen-error" role="alert">
          <p>{errorMessage(page.error)}</p>
          <button type="button" onClick={() => void page.refetch()} disabled={page.isFetching}>Retry</button>
        </div>
      )}
      {data && (
        <>
          <section className="browse-hero" aria-labelledby="browse-title">
            <Artwork url={data.thumbnail_url} size={200} />
            <div className="browse-summary">
              <h1 id="browse-title" ref={albumTitle} tabIndex={isAlbum ? -1 : undefined}>{data.title}</h1>
              {data.subtitle && <p>{data.subtitle}</p>}
              {data.second_subtitle && <p>{data.second_subtitle}</p>}
              <div className="browse-actions">
                <button
                  type="button"
                  className="primary browse-play"
                  aria-label="Play"
                  title="Play"
                  onClick={() => void playFrom(0)}
                  disabled={songs.length === 0}
                >
                  <PlayIcon />
                </button>
                <button type="button" className="ghost browse-shuffle" onClick={() => void shuffle()} disabled={songs.length === 0}>
                  <ShuffleIcon />
                  Shuffle
                </button>
              </div>
            </div>
          </section>
          {songs.length === 0 ? (
            <p className="hint">No tracks to play.</p>
          ) : (
            <ol className="rows" aria-label="Tracks">
              {songs.map((song, index) => (
                <li key={`${index}-${song.video_id}`}>
                  <SongMenu
                    className={`item item-row browse-track item-with-links${index === current ? " is-current" : ""}`}
                    song={song}
                    onOpenArtist={onOpenArtist}
                    onOpenAlbum={isAlbum ? () => albumTitle.current?.focus() : undefined}
                  >
                    <button
                      type="button"
                      className="item-action"
                      aria-label={[song.title, artistNames(song.artists)].filter(Boolean).join(", ")}
                      aria-current={index === current ? "true" : undefined}
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
        </>
      )}
    </>
  );
}
