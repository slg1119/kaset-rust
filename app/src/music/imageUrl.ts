/** Hosts YouTube Music serves artwork from; `tauri.conf.json`'s CSP allows the same. */
const ARTWORK_HOST = /\.(ytimg|ggpht|googleusercontent)\.com$/;

/**
 * The artwork URL if it is https on an allowed host (protocol-relative URLs
 * become https); otherwise `null`, so the UI shows a placeholder instead of
 * an image the CSP would block.
 */
export function safeImageUrl(url: string | null): string | null {
  if (url === null) return null;
  let parsed: URL;
  try {
    parsed = new URL(url.startsWith("//") ? `https:${url}` : url);
  } catch {
    return null;
  }
  return parsed.protocol === "https:" && ARTWORK_HOST.test(parsed.hostname) ? parsed.href : null;
}
