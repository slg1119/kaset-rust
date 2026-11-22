import { act, fireEvent, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { shortcutFor } from "./shortcuts";
import { useShortcuts } from "./useShortcuts";

const body = document.body;

function key(key: string, init: Partial<Parameters<typeof shortcutFor>[0]> = {}) {
  return {
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    target: body,
    ...init,
  };
}

function element(html: string): Element {
  const wrapper = document.createElement("div");
  wrapper.innerHTML = html;
  return wrapper.firstElementChild as Element;
}

test("maps macOS shortcuts", () => {
  expect(shortcutFor(key(" "), true)).toBe("togglePlayback");
  expect(shortcutFor(key("ArrowRight", { metaKey: true }), true)).toBe("nextTrack");
  expect(shortcutFor(key("ArrowLeft", { metaKey: true }), true)).toBe("previousTrack");
  expect(shortcutFor(key("1", { metaKey: true }), true)).toBe("goHome");
  expect(shortcutFor(key("f", { metaKey: true }), true)).toBe("goSearch");
  expect(shortcutFor(key("[", { metaKey: true }), true)).toBe("back");
  expect(shortcutFor(key("Escape"), true)).toBe("back");
  expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), true)).toBe(null);
});

test("uses Ctrl elsewhere", () => {
  expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), false)).toBe("nextTrack");
  expect(shortcutFor(key("f", { ctrlKey: true }), false)).toBe("goSearch");
  expect(shortcutFor(key("ArrowRight", { metaKey: true }), false)).toBe(null);
});

test("leaves system and unrelated shortcuts alone", () => {
  for (const name of ["q", "w", "h", "m", ",", "r", "s"]) {
    expect(shortcutFor(key(name, { metaKey: true }), true)).toBe(null);
  }
  expect(shortcutFor(key("f", { metaKey: true, shiftKey: true }), true)).toBe(null);
  expect(shortcutFor(key(" ", { shiftKey: true }), true)).toBe(null);
});

test("does not steal keys from controls", () => {
  expect(shortcutFor(key(" ", { target: element('<div role="option">Song</div>') }), true)).toBe(null);
  expect(shortcutFor(key(" ", { target: element('<input type="search">') }), true)).toBe(null);
  expect(shortcutFor(key(" ", { target: element('<input type="range">') }), true)).toBe(null);
  expect(shortcutFor(key("ArrowLeft", { metaKey: true, target: element("<textarea></textarea>") }), true)).toBe(
    null,
  );
  // ⌘F still reaches Search from a text field.
  expect(shortcutFor(key("f", { metaKey: true, target: element("<input>") }), true)).toBe("goSearch");
});

test("ignores held, composing, and handled keys", () => {
  expect(shortcutFor(key(" ", { repeat: true }), true)).toBe(null);
  expect(shortcutFor(key(" ", { isComposing: true }), true)).toBe(null);
  expect(shortcutFor(key(" ", { defaultPrevented: true }), true)).toBe(null);
});

test.each(["<button>Play</button>", '<a href="#home">Home</a>'])(
  "Space toggles playback unless the control has keyboard focus (%s)",
  (html) => {
    const target = element(html);
    const focusVisible = vi.spyOn(target, "matches").mockReturnValue(false);
    expect(shortcutFor(key(" ", { target }), true)).toBe("togglePlayback");
    expect(focusVisible).toHaveBeenCalledWith(":focus-visible");
    focusVisible.mockReturnValue(true);
    expect(shortcutFor(key(" ", { target }), true)).toBe(null);
    focusVisible.mockImplementation(() => {
      throw new Error("unsupported selector");
    });
    expect(shortcutFor(key(" ", { target }), true)).toBe("togglePlayback");
  },
);

test("the Space shortcut prevents a mouse-focused button from also activating", () => {
  const togglePlayback = vi.fn();
  const noop = vi.fn();
  renderHook(() => useShortcuts({
    togglePlayback,
    nextTrack: noop,
    previousTrack: noop,
    goHome: noop,
    goSearch: noop,
  }));
  const button = element("<button>Play</button>") as HTMLButtonElement;
  const focusVisible = vi.spyOn(button, "matches").mockReturnValue(false);
  body.append(button);
  function pressSpace() {
    const event = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");
    act(() => {
      button.dispatchEvent(event);
    });
    return preventDefault;
  }

  try {
    button.focus();
    // WebKit promotes :focus-visible before dispatching the first Space keydown.
    focusVisible.mockReturnValue(true);
    expect(pressSpace()).toHaveBeenCalledOnce();
    expect(togglePlayback).toHaveBeenCalledOnce();

    button.blur();
    button.focus();
    expect(pressSpace()).not.toHaveBeenCalled();
    expect(togglePlayback).toHaveBeenCalledOnce();

    // A pointer press on the already-focused button may not fire focusin again.
    fireEvent.pointerDown(button);
    expect(pressSpace()).toHaveBeenCalledOnce();
    expect(togglePlayback).toHaveBeenCalledTimes(2);
  } finally {
    button.remove();
  }
});
