import { useIsMutating } from "@tanstack/react-query";
import { commands, type LikeStatus, type SongActionStatus } from "../bindings";
import { HeartFilledIcon, HeartIcon, RefreshIcon, ThumbDownIcon } from "../icons";
import { songLibraryQueries, songMutationKey, useAccountMutation, useSongMetadata } from "../music/accountActions";
import { queryKeys } from "../query";

export function TrackRating({ videoId }: { videoId: string }) {
  const metadata = useSongMetadata(videoId);
  const key = queryKeys.songMetadata(videoId);
  const mutationKey = songMutationKey(videoId);
  const updating = useIsMutating({ mutationKey, exact: true }) > 0;
  const mutation = useAccountMutation({
    mutationKey,
    request: (rating: LikeStatus) => commands.rateSong(videoId, rating),
    optimistic: { key, update: (data: SongActionStatus, rating) => ({ ...data, like_status: rating }) },
    invalidate: () => [key, ...songLibraryQueries],
  });
  const rating = metadata.data?.like_status;
  const disabled = !metadata.data || updating;
  if (metadata.isError && !metadata.data) {
    return (
      <button type="button" className="icon-button" aria-label="Retry loading rating" title="Retry loading rating"
        disabled={metadata.isFetching} onClick={() => { void metadata.refetch(); }}>
        <RefreshIcon />
      </button>
    );
  }

  return (
    <>
      <button type="button" className="icon-button" aria-label="Like" title="Like"
        aria-pressed={rating === "LIKE"} disabled={disabled}
        onClick={() => mutation.mutate(rating === "LIKE" ? "INDIFFERENT" : "LIKE")}>
        {rating === "LIKE" ? <HeartFilledIcon /> : <HeartIcon />}
      </button>
      <button type="button" className="icon-button" aria-label="Dislike" title="Dislike"
        aria-pressed={rating === "DISLIKE"} disabled={disabled}
        onClick={() => mutation.mutate(rating === "DISLIKE" ? "INDIFFERENT" : "DISLIKE")}>
        <ThumbDownIcon />
      </button>
    </>
  );
}
