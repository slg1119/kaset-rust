import { useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { commands, type Episode } from "../bindings";
import { errorMessage } from "../errors";
import { ChevronLeftIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { episodeToSong } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { BROWSE_PAGE_STALE_MS, nextPageParam, queryKeys } from "../query";

const MAX_QUEUE_LENGTH = 500;

type Props = {
  browseId: string;
  title: string;
  onBack: () => void;
};

export function PodcastShowScreen({ browseId, title, onBack }: Props) {
  const show = useInfiniteQuery({
    queryKey: queryKeys.podcastShow(browseId),
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) => pageParam === null
      ? { kind: "show" as const, ...await commands.getPodcastShow(browseId) }
      : { kind: "episodes" as const, ...await commands.getPodcastEpisodesContinuation(pageParam) },
    getNextPageParam: nextPageParam,
    staleTime: BROWSE_PAGE_STALE_MS,
  });
  const play = usePlayQueue();
  const backButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    backButton.current?.focus();
  }, []);

  const firstPage = show.data?.pages[0];
  const data = firstPage?.kind === "show" ? firstPage : undefined;
  const showTitle = data?.title.trim() ? data.title : title;
  const episodesById = new Map<string, Episode>();
  for (const page of show.data?.pages ?? []) {
    for (const episode of page.episodes) {
      if (!episodesById.has(episode.video_id)) episodesById.set(episode.video_id, episode);
    }
  }
  const episodes = [...episodesById.values()];

  function playFrom(index: number) {
    const offset = index < MAX_QUEUE_LENGTH ? 0 : index;
    const songs = episodes.slice(offset, offset + MAX_QUEUE_LENGTH)
      .map((episode) => episodeToSong(episode, showTitle, data?.thumbnail_url));
    return play(songs, index - offset);
  }

  return (
    <>
      <header className="content-header browse-header">
        <button ref={backButton} type="button" className="icon-button" aria-label="Back" title="Back" onClick={onBack}>
          <ChevronLeftIcon />
        </button>
        <span>{showTitle}</span>
      </header>
      {show.isPending && <p className="hint" role="status">Loading…</p>}
      {show.isError && !show.isFetchNextPageError && (
        <div className="screen-error" role="alert">
          <p>{errorMessage(show.error)}</p>
          <button type="button" onClick={() => void show.refetch()} disabled={show.isFetching}>Retry</button>
        </div>
      )}
      {data && (
        <>
          <section className="browse-hero" aria-labelledby="podcast-title">
            <Artwork url={data.thumbnail_url} size={200} />
            <div className="browse-summary">
              <h1 id="podcast-title">{showTitle}</h1>
              {data.author && <p>{data.author}</p>}
              {data.description && <p>{data.description}</p>}
            </div>
          </section>
          {episodes.length === 0 && !show.hasNextPage && <p className="hint">No episodes to play.</p>}
          {episodes.length > 0 && (
            <ol className="rows" aria-label="Episodes">
              {episodes.map((episode, index) => (
                <li key={episode.video_id}>
                  <button
                    type="button"
                    className="item item-row"
                    aria-label={`Play ${episode.title}`}
                    onClick={() => void playFrom(index)}
                  >
                    <Artwork url={episode.thumbnail_url ?? data.thumbnail_url} size={56} />
                    <span className="item-text">
                      <span className="item-title">{episode.title}</span>
                      {(episode.date || episode.duration) && (
                        <span className="item-subtitle">{[episode.date, episode.duration].filter(Boolean).join(" · ")}</span>
                      )}
                      {episode.description && <span className="item-subtitle">{episode.description}</span>}
                    </span>
                  </button>
                </li>
              ))}
            </ol>
          )}
          {show.hasNextPage && (
            <div className="feed-more">
              {show.isFetchNextPageError && <p role="alert">{errorMessage(show.error)}</p>}
              {show.isFetchingNextPage && <p className="hint" role="status">Loading more episodes…</p>}
              <button
                type="button"
                className="ghost"
                disabled={show.isFetching}
                onClick={() => void show.fetchNextPage({ cancelRefetch: false })}
              >
                {show.isFetchNextPageError ? "Retry loading episodes" : "Load more episodes"}
              </button>
            </div>
          )}
        </>
      )}
    </>
  );
}
