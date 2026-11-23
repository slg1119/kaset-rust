import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "../bindings";
import { isMac } from "../shell/shortcuts";
import { renderWithProviders } from "../test/render";
import { PlayerBar } from "./PlayerBar";
import { idleSnapshot, playingState, testSong } from "./testing";

const timedLyrics = {
  lines: [
    { start_ms: 1000, end_ms: 2500, text: "First lyric" },
    { start_ms: 3250, end_ms: null, text: "Second lyric" },
  ],
  text: null,
  source: "YTMusic",
};

function renderLyrics(getLyrics = vi.fn(() => timedLyrics)) {
  const seekTo = vi.fn(() => null);
  const handlers = {
    getLyrics,
    seekTo,
    playerSnapshot: () => ({ state: playingState({ position: 0 }), queue: idleSnapshot.queue }),
  };
  mockCommands(handlers);
  renderWithProviders(<PlayerBar />);
  return { getLyrics, seekTo };
}

test("highlights the current lyric, clears gaps, and seeks in seconds", async () => {
  const { getLyrics, seekTo } = renderLyrics();
  const toggle = await screen.findByRole("button", { name: "Lyrics" });
  expect(getLyrics).not.toHaveBeenCalled();
  await userEvent.click(toggle);
  expect(getLyrics).toHaveBeenCalledWith({ videoId: testSong.video_id });
  const first = await screen.findByRole("button", { name: "First lyric" });
  const second = screen.getByRole("button", { name: "Second lyric" });
  expect(first).not.toHaveAttribute("aria-current");
  await act(() => events.playerState.emit({ state: playingState({ position: 1 }) }));
  expect(first).toHaveAttribute("aria-current", "true");
  await act(() => events.playerState.emit({ state: playingState({ position: 2.5 }) }));
  expect(first).not.toHaveAttribute("aria-current");
  await act(() => events.playerState.emit({ state: playingState({ position: 4 }) }));
  expect(second).toHaveAttribute("aria-current", "true");
  await userEvent.click(second);
  expect(seekTo).toHaveBeenCalledWith({ position: 3.25 });
});

test.each([
  { text: "Plain lyric\nSecond line", source: "Test provider", expected: /Plain lyric/ },
  { text: null, source: null, expected: "No lyrics" },
])("shows $expected and toggles using the shifted shortcut", async ({ text, source, expected }) => {
  const getLyrics = vi.fn(() => ({ lines: null, text, source }));
  const handlers = {
    getLyrics,
    playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }),
  };
  mockCommands(handlers);
  renderWithProviders(<PlayerBar />);
  const toggle = await screen.findByRole("button", { name: "Lyrics" });
  const primary = isMac() ? { metaKey: true } : { ctrlKey: true };
  fireEvent.keyDown(toggle, { key: "l", ...primary });
  expect(getLyrics).not.toHaveBeenCalled();
  fireEvent.keyDown(toggle, { key: "L", shiftKey: true, ...primary });
  expect(await screen.findByText(expected)).toBeInTheDocument();
  if (source) expect(screen.getByText(source)).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("region", { name: "Lyrics" })).not.toBeInTheDocument();
  expect(toggle).toHaveFocus();
});

test.each([false, true])("centers the active line with reduced motion set to %s", async (reduced) => {
  const matchMedia = vi.fn(() => ({ matches: reduced }));
  vi.stubGlobal("matchMedia", matchMedia);
  try {
    renderLyrics();
    await userEvent.click(await screen.findByRole("button", { name: "Lyrics" }));
    const first = await screen.findByRole("button", { name: "First lyric" });
    const row = first.closest("li");
    if (!row) throw new Error("Expected a lyric list item");
    row.scrollIntoView = vi.fn();
    await act(() => events.playerState.emit({ state: playingState({ position: 1 }) }));
    expect(matchMedia).toHaveBeenCalledWith("(prefers-reduced-motion: reduce)");
    expect(row.scrollIntoView).toHaveBeenCalledExactlyOnceWith({
      block: "center", behavior: reduced ? "instant" : "smooth",
    });
  } finally {
    vi.unstubAllGlobals();
  }
});

test("retries a failed load and switches lyrics when the track changes", async () => {
  const getLyrics = vi.fn<(args: { videoId: string }) => { lines: null; text: string; source: null }>()
    .mockImplementationOnce(() => { throw { kind: "network", message: "Connection lost" }; })
    .mockImplementation(({ videoId }) => ({ lines: null, text: `Lyrics for ${videoId}`, source: null }));
  const handlers = {
    getLyrics,
    playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }),
  };
  mockCommands(handlers);
  renderWithProviders(<PlayerBar />);
  await userEvent.click(await screen.findByRole("button", { name: "Lyrics" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Connection lost");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText(`Lyrics for ${testSong.video_id}`)).toBeInTheDocument();

  await act(() => events.playerState.emit({
    state: playingState({ track: { ...testSong, video_id: "test-vid-02" } }),
  }));
  expect(await screen.findByText("Lyrics for test-vid-02")).toBeInTheDocument();
  expect(screen.queryByText(`Lyrics for ${testSong.video_id}`)).not.toBeInTheDocument();
});
