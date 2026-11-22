import { useEffect, useEffectEvent } from "react";
import { hasKeyboardFocus, shortcutFor, type Shortcut } from "./shortcuts";

/** Runs `actions[shortcut]` for app shortcuts pressed anywhere in the window. */
export function useShortcuts(actions: Record<Shortcut, () => void>) {
  const run = useEffectEvent((shortcut: Shortcut) => actions[shortcut]());

  useEffect(() => {
    // WebKit can set :focus-visible before dispatching Space. Read it when
    // focus changes so the key itself cannot change how the focus was acquired.
    let focused = document.activeElement;
    let focusVisible = hasKeyboardFocus(focused);

    function onFocusIn(event: FocusEvent) {
      focused = event.target instanceof Element ? event.target : null;
      focusVisible = hasKeyboardFocus(focused);
    }

    function onPointerDown(event: PointerEvent) {
      // Pressing the same control can retain focus without a focusin event.
      if (event.target instanceof Node && focused?.contains(event.target)) focusVisible = false;
    }

    function onKeyDown(event: KeyboardEvent) {
      const shortcut = shortcutFor(event, undefined, event.target === focused ? focusVisible : undefined);
      if (shortcut === null) return;
      event.preventDefault();
      run(shortcut);
    }

    window.addEventListener("focusin", onFocusIn, true);
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("focusin", onFocusIn, true);
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
}
