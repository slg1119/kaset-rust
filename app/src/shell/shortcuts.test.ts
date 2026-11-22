import { expect, test } from "vitest";
import { shortcutFor } from "./shortcuts";

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
  expect(shortcutFor(key(" ", { target: element("<button>Play</button>") }), true)).toBe(null);
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
