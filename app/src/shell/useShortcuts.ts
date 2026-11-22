import { useEffect } from "react";
import { shortcutFor, type Shortcut } from "./shortcuts";

/** Runs `actions[shortcut]` for app shortcuts pressed anywhere in the window. */
export function useShortcuts(actions: Record<Shortcut, () => void>) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const shortcut = shortcutFor(event);
      if (shortcut === null) return;
      event.preventDefault();
      actions[shortcut]();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [actions]);
}
