/** `m:ss` for a position in seconds. Rust sends `null` for a non-finite number. */
export function formatTime(seconds: number | null): string {
  const total = seconds !== null && Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  return `${minutes}:${rest.toString().padStart(2, "0")}`;
}
