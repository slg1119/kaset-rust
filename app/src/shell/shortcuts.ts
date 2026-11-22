export type Shortcut = "togglePlayback" | "nextTrack" | "previousTrack" | "goHome" | "goSearch" | "back";

type KeyInput = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "repeat" | "isComposing" | "defaultPrevented"
> & { target: EventTarget | null };

const EDITABLE = "input, textarea, select, [contenteditable=''], [contenteditable='true']";
const SPACE_CONTROLS = `${EDITABLE}, [role='slider'], [role='option']`;

export function hasKeyboardFocus(target: EventTarget | null): boolean {
  const control = target instanceof Element ? target.closest("button, a[href]") : null;
  try {
    return control?.matches(":focus-visible") ?? false;
  } catch {
    return false;
  }
}

function matches(target: EventTarget | null, selector: string): boolean {
  return target instanceof Element && target.closest(selector) !== null;
}

export function isMac(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.userAgent);
}

/**
 * Maps a key press to an app shortcut, following macOS and Apple Music:
 * Space play/pause, ⌘→ next, ⌘← previous, ⌘1 Home, ⌘F Search (Ctrl instead
 * of ⌘ elsewhere). Escape and ⌘[ go back when a detail page is open. ⌘Q, ⌘W, ⌘H, ⌘M, and ⌘, are never taken.
 */
export function shortcutFor(event: KeyInput, mac: boolean = isMac(), focusVisible?: boolean): Shortcut | null {
  if (event.defaultPrevented || event.isComposing) return null;
  const anyModifier = event.metaKey || event.ctrlKey || event.altKey || event.shiftKey;
  if (event.key === " " && !anyModifier) {
    return event.repeat || matches(event.target, SPACE_CONTROLS) || (focusVisible ?? hasKeyboardFocus(event.target))
      ? null
      : "togglePlayback";
  }
  if (event.key === "Escape" && !anyModifier && !event.repeat) return "back";
  const primary = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!primary || event.altKey || event.shiftKey) return null;
  switch (event.key) {
    case "[":
      return event.repeat ? null : "back";
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
