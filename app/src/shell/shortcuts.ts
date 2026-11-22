export type Shortcut = "togglePlayback" | "nextTrack" | "previousTrack" | "goHome" | "goSearch";

type KeyInput = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "repeat" | "isComposing" | "defaultPrevented"
> & { target: EventTarget | null };

/** Controls where keys already mean something (typing, activating, sliding). */
const INTERACTIVE =
  "input, textarea, select, button, a[href], [contenteditable=''], [contenteditable='true'], [role='slider'], [role='option']";
const EDITABLE = "input, textarea, select, [contenteditable=''], [contenteditable='true']";

function matches(target: EventTarget | null, selector: string): boolean {
  return target instanceof Element && target.closest(selector) !== null;
}

export function isMac(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.userAgent);
}

/**
 * Maps a key press to an app shortcut, following macOS and Apple Music:
 * Space play/pause, ⌘→ next, ⌘← previous, ⌘1 Home, ⌘F Search (Ctrl instead
 * of ⌘ elsewhere). ⌘Q, ⌘W, ⌘H, ⌘M, and ⌘, are never taken.
 */
export function shortcutFor(event: KeyInput, mac: boolean = isMac()): Shortcut | null {
  if (event.defaultPrevented || event.isComposing) return null;
  const anyModifier = event.metaKey || event.ctrlKey || event.altKey || event.shiftKey;
  if (event.key === " " && !anyModifier) {
    return event.repeat || matches(event.target, INTERACTIVE) ? null : "togglePlayback";
  }
  const primary = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!primary || event.altKey || event.shiftKey) return null;
  switch (event.key) {
    case "ArrowRight":
      return matches(event.target, EDITABLE) ? null : "nextTrack";
    case "ArrowLeft":
      return matches(event.target, EDITABLE) ? null : "previousTrack";
    case "1":
      return "goHome";
    case "f":
    case "F":
      return "goSearch";
    default:
      return null;
  }
}
