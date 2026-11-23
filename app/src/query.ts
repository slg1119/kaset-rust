import { QueryClient } from "@tanstack/react-query";
import type { FeedKind, SearchFilter } from "./bindings";
import { isAppError } from "./errors";

/** How long fetched data counts as fresh before a remount refetches it. */
export const FEED_STALE_MS = 5 * 60_000;
export const SEARCH_STALE_MS = 10 * 60_000;
export const BROWSE_PAGE_STALE_MS = 10 * 60_000;
export const SUGGESTIONS_STALE_MS = 60_000;
export const LYRICS_STALE_MS = 10 * 60_000;
export const ACCOUNT_STALE_MS = 30 * 60_000;

export const queryKeys = {
  feed: (kind: FeedKind | "podcasts") => ["feed", kind] as const,
  feedCategory: (browseId: string, params: string | null) => ["feed", "category", browseId, params] as const,
  feedChip: (params: string | null) => ["feed", "chip", params] as const,
  search: (query: string, filter: SearchFilter | null) => ["search", query, filter] as const,
  artist: (browseId: string) => ["artist", browseId] as const,
  artistMore: (browseId: string, params: string | null) => ["artist", browseId, "more", params] as const,
  browsePage: (browseId: string) => ["browsePage", browseId] as const,
  podcastShow: (browseId: string) => ["podcastShow", browseId] as const,
  suggestions: (input: string) => ["suggestions", input] as const,
  account: ["account"] as const,
  lyrics: (videoId: string) => ["lyrics", videoId] as const,
};

type ContinuablePage = { continuation: string | null };

export function nextPageParam<Page extends ContinuablePage>(
  page: Page,
  _pages: Page[],
  _pageParam: string | null,
  used: (string | null)[],
) {
  const token = page.continuation;
  return token && !used.includes(token) ? token : undefined;
}

/**
 * Retries once after a network failure or an unexpected HTTP status. Auth,
 * rate-limit, parse, and argument errors fail at once: retrying cannot fix
 * them, and an auth error already switched the app to the sign-in screen.
 */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  return failureCount < 1 && isAppError(error) && (error.kind === "network" || error.kind === "http");
}

export function createQueryClient({ retry = true }: { retry?: boolean } = {}): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: retry ? shouldRetry : false,
        // A desktop window gains focus constantly; refetch on remount or Refresh instead.
        refetchOnWindowFocus: false,
      },
    },
  });
}
