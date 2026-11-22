import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands, type ArtistPage, type BrowseEndpoint, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { FeedShelf, type CategoryItem } from "../home/FeedShelf";
import { ChevronLeftIcon, PlayIcon } from "../icons";
import type { OpenArtist } from "../music/ArtistNames";
import { Artwork } from "../music/Artwork";
import { isPlayable, toSong, type PageCard } from "../music/items";
import { useNotice } from "../notices/NoticeProvider";
import { MAX_QUEUE_LENGTH, usePlayQueue } from "../player/usePlayQueue";
import { BROWSE_PAGE_STALE_MS, queryKeys } from "../query";

type Navigation = {
  onBack: () => void;
  onOpenPage: (card: PageCard) => void;
  onOpenCategory: (item: CategoryItem) => void;
  onOpenArtist: OpenArtist;
};

type ArtistProps = Navigation & {
  browseId: string;
  name: string;
  onOpenMore: (endpoint: BrowseEndpoint, title: string) => void;
};

export function ArtistScreen({ browseId, name, onBack, onOpenMore, ...navigation }: ArtistProps) {
  const page = useQuery({
    queryKey: queryKeys.artist(browseId),
    queryFn: () => commands.getArtist(browseId),
    staleTime: BROWSE_PAGE_STALE_MS,
  });
  const play = usePlayQueue();
  const artist = page.data;
  const topSongs = artist && artist.top_songs_section !== null ? artist.sections[artist.top_songs_section] : undefined;

  return (
    <>
      <ArtistHeader title={artist?.name ?? name} onBack={onBack} heading={!artist} />
      {page.isPending && <p className="hint" role="status">Loading…</p>}
      {page.isError && (
        <LoadError error={page.error} retry={() => void page.refetch()} fetching={page.isFetching} />
      )}
      {artist && (
        <>
          <ArtistHero artist={artist} play={play} topSongs={topSongs?.items.filter(isPlayable).map(toSong) ?? []} />
          {artist.sections.map((section, index) => (
            <FeedShelf key={`${index}-${section.title}`} section={section} onPlay={play}
              layout={section === topSongs ? "quick-picks" : "carousel"} onOpenMore={onOpenMore} {...navigation} />
          ))}
        </>
      )}
    </>
  );
}

function ArtistHero({ artist, play, topSongs }: {
  artist: ArtistPage;
  play: ReturnType<typeof usePlayQueue>;
  topSongs: Song[];
}) {
  const { report, dismiss } = useNotice();
  const [starting, setStarting] = useState(false);
  const hasRadio = artist.radio_playlist_id || artist.radio_video_id;
  const playlistId = hasRadio ? artist.radio_playlist_id : artist.shuffle_playlist_id;
  const videoId = hasRadio ? artist.radio_video_id : artist.shuffle_video_id;
  const params = hasRadio ? artist.radio_params : artist.shuffle_params;

  async function start() {
    if (!playlistId && !videoId) {
      await play(topSongs.slice(0, MAX_QUEUE_LENGTH), 0);
      return;
    }
    dismiss();
    setStarting(true);
    try {
      await commands.playArtistMix(playlistId, videoId, params);
    } catch (cause) {
      report(errorMessage(cause));
    } finally {
      setStarting(false);
    }
  }

  return (
    <section className="artist-hero" aria-labelledby="artist-title">
      <div className="artist-hero-art"><Artwork url={artist.thumbnail_url} size="fill" /></div>
      <div className="artist-summary">
        <h1 id="artist-title">{artist.name}</h1>
        {artist.subscribers && <p>{artist.subscribers}</p>}
        {artist.description && <p className="artist-description">{artist.description}</p>}
        <button type="button" className="primary artist-play" onClick={() => void start()}
          disabled={starting || (!playlistId && !videoId && topSongs.length === 0)}>
          <PlayIcon size={18} />
          {starting ? "Starting…" : "Play"}
        </button>
      </div>
    </section>
  );
}

export function ArtistMoreScreen({ browseId, params, title, onBack, ...navigation }: Navigation & {
  browseId: string;
  params: string | null;
  title: string;
}) {
  const page = useQuery({
    queryKey: queryKeys.artistMore(browseId, params),
    queryFn: () => commands.getArtistMore(browseId, params),
    staleTime: BROWSE_PAGE_STALE_MS,
  });
  const play = usePlayQueue();

  return (
    <>
      <ArtistHeader title={title} onBack={onBack} heading />
      {page.isPending && <p className="hint" role="status">Loading…</p>}
      {page.isError && (
        <LoadError error={page.error} retry={() => void page.refetch()} fetching={page.isFetching} />
      )}
      {page.data?.length === 0 && <p className="hint">Nothing to show yet.</p>}
      {page.data && page.data.length > 0 && (
        <FeedShelf section={{ title, items: page.data }} onPlay={play}
          layout={page.data.every(isPlayable) ? "rows" : "grid"} {...navigation} />
      )}
    </>
  );
}

function ArtistHeader({ title, heading, onBack }: { title: string; heading?: boolean; onBack: () => void }) {
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => button.current?.focus(), []);
  return (
    <header className="content-header browse-header">
      <button ref={button} type="button" className="icon-button" aria-label="Back" title="Back" onClick={onBack}>
        <ChevronLeftIcon />
      </button>
      {heading ? <h1>{title}</h1> : <span>{title}</span>}
    </header>
  );
}

function LoadError({ error, retry, fetching }: { error: unknown; retry: () => void; fetching: boolean }) {
  return (
    <div className="screen-error" role="alert">
      <p>{errorMessage(error)}</p>
      <button type="button" onClick={retry} disabled={fetching}>Retry</button>
    </div>
  );
}
