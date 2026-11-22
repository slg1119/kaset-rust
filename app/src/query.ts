import { QueryClient } from "@tanstack/react-query";
import { isAppError } from "./errors";

/** How long fetched data counts as fresh before a remount refetches it. */
export const HOME_STALE_MS = 5 * 60_000;
export const SEARCH_STALE_MS = 10 * 60_000;
export const SUGGESTIONS_STALE_MS = 60_000;
export const ACCOUNT_STALE_MS = 30 * 60_000;

export const queryKeys = {
  home: ["home"] as const,
  search: (query: string) => ["search", query] as const,
  suggestions: (input: string) => ["suggestions", input] as const,
  account: ["account"] as const,
};

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
