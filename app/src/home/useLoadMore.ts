import { useEffect, useRef } from "react";

/** Observes the end of a scrollable screen while it can load another page. */
export function useLoadMore(
  enabled: boolean,
  pageCount: number,
  fetchNextPage: (options: { cancelRefetch: boolean }) => Promise<unknown>,
) {
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const target = sentinel.current;
    if (!enabled || !target || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) void fetchNextPage({ cancelRefetch: false });
    }, { root: target.closest("[role=main], main"), rootMargin: "0px 0px 160px" });
    observer.observe(target);
    return () => observer.disconnect();
  // Start observing again after an append, even when the sentinel stays visible.
  }, [enabled, fetchNextPage, pageCount]);
  return sentinel;
}
