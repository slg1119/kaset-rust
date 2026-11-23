import { useMutation, useQuery, useQueryClient, type QueryKey } from "@tanstack/react-query";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { useNotice } from "../notices/NoticeProvider";
import { queryKeys } from "../query";

export const songMutationKey = (videoId: string) => ["songAction", videoId] as const;

export const songLibraryQueries = [queryKeys.library("songs"), queryKeys.browsePage("VLLM")];

export function barePlaylistId(id: string) {
  return id.startsWith("VL") ? id.slice(2) : id;
}

export function playlistQueries(id: string) {
  const bare = barePlaylistId(id);
  return [queryKeys.browsePage(bare), queryKeys.browsePage(`VL${bare}`)];
}

export function useSongMetadata(videoId: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.songMetadata(videoId),
    queryFn: () => commands.songMetadata(videoId),
    staleTime: 60_000,
    enabled,
  });
}

export function useAccountMutation<Variables, Result, Data = never>({
  mutationKey,
  request,
  invalidate,
  optimistic,
}: {
  mutationKey?: QueryKey;
  request: (variables: Variables) => Promise<Result>;
  invalidate: (variables: Variables) => QueryKey[];
  optimistic?: { key: QueryKey; update: (data: Data, variables: Variables) => Data };
}) {
  const client = useQueryClient();
  const { report, dismiss } = useNotice();
  return useMutation({
    mutationKey,
    mutationFn: request,
    async onMutate(variables) {
      dismiss();
      if (!optimistic) return;
      await client.cancelQueries({ queryKey: optimistic.key, exact: true });
      const previous = client.getQueryData<Data>(optimistic.key);
      if (previous !== undefined) {
        client.setQueryData(optimistic.key, optimistic.update(previous, variables));
      }
      return previous;
    },
    onError(cause, _variables, previous) {
      if (optimistic && previous !== undefined) client.setQueryData(optimistic.key, previous);
      report(errorMessage(cause));
    },
    onSuccess: (_, variables) => Promise.all(
      invalidate(variables).map((queryKey) => client.invalidateQueries({ queryKey })),
    ),
  });
}
