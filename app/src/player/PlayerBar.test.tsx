import { act, fireEvent, renderHook, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "../bindings";
import { renderWithProviders } from "../test/render";
import { formatTime } from "./format";
import { PlayerBar } from "./PlayerBar";
import { idleSnapshot, playingState, testSong } from "./testing";
import { PlayerProvider } from "./PlayerProvider";
import { usePlayerSubscription } from "./usePlayer";

function renderPlaying(overrides: Parameters<typeof playingState>[0] = {}) {
  const handlers = {
    playerSnapshot: () => ({
      state: playingState(overrides),
      queue: { songs: [testSong, testSong], current: 0, browse_track: null },
    }),
    togglePlayback: vi.fn(() => null),
    nextTrack: vi.fn(() => null),
    previousTrack: vi.fn(() => null),
    seekTo: vi.fn(() => null),
    setRepeat: vi.fn(() => null),
    setShuffle: vi.fn(() => null),
  };
  mockCommands(handlers);
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  return handlers;
}

test("formats times as m:ss", () => {
  expect(formatTime(0)).toBe("0:00");
  expect(formatTime(65.9)).toBe("1:05");
  expect(formatTime(3600)).toBe("60:00");
  expect(formatTime(null)).toBe("0:00");
  expect(formatTime(-3)).toBe("0:00");
});

test("stays hidden while nothing is queued", async () => {
  const playerSnapshot = vi.fn(() => idleSnapshot);
  mockCommands({ playerSnapshot });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  await vi.waitFor(() => expect(playerSnapshot).toHaveBeenCalled());
  expect(screen.queryByRole("contentinfo", { name: "Player" })).not.toBeInTheDocument();
});

test("shows the track from player://state", async () => {
  mockCommands({ playerSnapshot: () => idleSnapshot });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  await act(() => events.playerState.emit({ state: playingState() }));
  expect(await screen.findByText("Test song")).toBeInTheDocument();
  expect(screen.getByText("Test artist")).toBeInTheDocument();
  expect(screen.getByText("1:05")).toBeInTheDocument();
  expect(screen.getByText("3:20")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
});

test("follows player://queue for the position in the queue", async () => {
  renderPlaying();
  expect(await screen.findByText("1 of 2")).toBeInTheDocument();
  await act(() =>
    events.playerQueue.emit({ queue: { songs: [testSong, testSong, testSong], current: 2, browse_track: null } }),
  );
  expect(await screen.findByText("3 of 3")).toBeInTheDocument();
});

test("transport buttons call the player commands", async () => {
  const handlers = renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: "Pause" }));
  await userEvent.click(screen.getByRole("button", { name: "Next" }));
  await userEvent.click(screen.getByRole("button", { name: "Previous" }));
  expect(handlers.togglePlayback).toHaveBeenCalledTimes(1);
  expect(handlers.nextTrack).toHaveBeenCalledTimes(1);
  expect(handlers.previousTrack).toHaveBeenCalledTimes(1);
});

test("next is disabled while paused with no upcoming songs", async () => {
  renderPlaying({ has_next: false, status: "paused" });
  expect(await screen.findByRole("button", { name: "Next" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
});

test("next remains available for an autoplay retry once the queue has ended", async () => {
  const { nextTrack } = renderPlaying({ has_next: false, status: "ended" });
  await userEvent.click(await screen.findByRole("button", { name: "Next" }));
  expect(nextTrack).toHaveBeenCalledTimes(1);
});

test("repeat cycles off, all, one and shuffle toggles", async () => {
  const handlers = renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: "Repeat off" }));
  expect(handlers.setRepeat).toHaveBeenLastCalledWith({ mode: "all" });
  await act(() => events.playerState.emit({ state: playingState({ repeat: "one", shuffle: true }) }));
  await userEvent.click(await screen.findByRole("button", { name: "Repeat one" }));
  expect(handlers.setRepeat).toHaveBeenLastCalledWith({ mode: "off" });
  const shuffle = screen.getByRole("button", { name: "Shuffle" });
  expect(shuffle).toHaveAttribute("aria-pressed", "true");
  await userEvent.click(shuffle);
  expect(handlers.setShuffle).toHaveBeenLastCalledWith({ enabled: false });
});

test("seeking commits when the slider is released", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  expect(handlers.seekTo).not.toHaveBeenCalled();
  expect(screen.getByText("2:00")).toBeInTheDocument();
  fireEvent.pointerUp(slider);
  expect(handlers.seekTo).toHaveBeenCalledWith({ position: 120 });
});

test("an ad replaces the timeline", async () => {
  renderPlaying({ is_ad: true });
  expect(await screen.findByText("Ad")).toBeInTheDocument();
  expect(screen.queryByRole("slider", { name: "Seek" })).not.toBeInTheDocument();
});

test("command errors are shown", async () => {
  mockCommands({
    playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }),
    nextTrack: () => {
      throw { kind: "internal", message: "player window failed" };
    },
  });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  await userEvent.click(await screen.findByRole("button", { name: "Next" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("player window failed");
});

test("the snapshot waits for both listeners, and earlier events win", async () => {
  let finishRegistration: ((stop: () => void) => void) | undefined;
  const listenQueue = vi.spyOn(events.playerQueue, "listen").mockImplementation(
    () => new Promise((resolve) => (finishRegistration = resolve)),
  );
  let reply: ((snapshot: typeof idleSnapshot) => void) | undefined;
  const playerSnapshot = vi.fn(() => new Promise<typeof idleSnapshot>((resolve) => (reply = resolve)));
  mockCommands({ playerSnapshot });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  await act(async () => expect(listenQueue).toHaveBeenCalledOnce());
  expect(playerSnapshot).not.toHaveBeenCalled();
  await act(async () => finishRegistration?.(vi.fn()));
  await vi.waitFor(() => expect(reply).toBeDefined());
  await act(() => events.playerState.emit({ state: playingState() }));
  await act(async () => reply?.(idleSnapshot));
  expect(screen.getByText("Test song")).toBeInTheDocument();
});

test("a failed snapshot is reported", async () => {
  mockCommands({
    playerSnapshot: () => {
      throw { kind: "internal", message: "player unavailable" };
    },
  });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Could not read the player state: player unavailable",
  );
});

test("keyboard seeking commits on key release", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "90" } });
  fireEvent.keyUp(slider, { key: "ArrowRight" });
  expect(handlers.seekTo).toHaveBeenCalledWith({ position: 90 });
});

test("a cancelled drag or blur drops the drag position without seeking", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  fireEvent.pointerCancel(slider);
  expect(screen.getByText("1:05")).toBeInTheDocument();
  fireEvent.change(slider, { target: { value: "150" } });
  fireEvent.blur(slider);
  expect(screen.getByText("1:05")).toBeInTheDocument();
  fireEvent.pointerUp(slider);
  expect(handlers.seekTo).not.toHaveBeenCalled();
});

test("a seek does not snap back to stale progress", async () => {
  renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  fireEvent.pointerUp(slider);
  // Progress sampled before the page applied the seek.
  await act(() => events.playerState.emit({ state: playingState({ position: 66 }) }));
  expect(screen.getByText("2:00")).toBeInTheDocument();
  // The page caught up.
  await act(() => events.playerState.emit({ state: playingState({ position: 121 }) }));
  expect(screen.getByText("2:01")).toBeInTheDocument();
});

test("a seek override ends after the settle time", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    renderPlaying();
    const slider = await screen.findByRole("slider", { name: "Seek" });
    fireEvent.change(slider, { target: { value: "120" } });
    fireEvent.pointerUp(slider);
    await act(() => events.playerState.emit({ state: playingState({ position: 66 }) }));
    expect(screen.getByText("2:00")).toBeInTheDocument();
    await act(async () => vi.advanceTimersByTime(2100));
    expect(screen.getByText("1:06")).toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});

test("unmounting stops listening to player events", async () => {
  const stopState = vi.fn();
  const stopQueue = vi.fn();
  vi.spyOn(events.playerState, "listen").mockResolvedValue(stopState);
  vi.spyOn(events.playerQueue, "listen").mockResolvedValue(stopQueue);
  const playerSnapshot = vi.fn(() => idleSnapshot);
  mockCommands({ playerSnapshot });
  const { unmount } = renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  await vi.waitFor(() => expect(playerSnapshot).toHaveBeenCalled());
  unmount();
  await vi.waitFor(() => expect(stopState).toHaveBeenCalledTimes(1));
  expect(stopQueue).toHaveBeenCalledTimes(1);
});

test("the queue button shows the queue with the current song marked", async () => {
  renderPlaying();
  const button = await screen.findByRole("button", { name: /Queue/ });
  expect(button).toHaveAttribute("aria-expanded", "false");
  await userEvent.click(button);
  expect(button).toHaveAttribute("aria-expanded", "true");
  const panel = screen.getByRole("region", { name: "Queue" });
  const items = within(panel).getAllByRole("listitem");
  expect(items).toHaveLength(2);
  expect(items[0]).toHaveAttribute("aria-current", "true");
  expect(items[1]).not.toHaveAttribute("aria-current");
  await userEvent.click(button);
  expect(screen.queryByRole("region", { name: "Queue" })).not.toBeInTheDocument();
});

test("publishes its height for the scrolling content", async () => {
  const observers: ResizeObserverCallback[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: ResizeObserverCallback) {
        observers.push(callback);
      }
      observe() {}
      disconnect() {}
    },
  );
  try {
    mockCommands({ playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }) });
    const { container, unmount } = renderWithProviders(
      <div className="content-column">
        <PlayerProvider><PlayerBar /></PlayerProvider>
      </div>,
    );
    await screen.findByRole("contentinfo", { name: "Player" });
    const column = container.querySelector(".content-column") as HTMLElement;
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(72);
    act(() => observers.forEach((callback) => callback([], {} as ResizeObserver)));
    expect(column.style.getPropertyValue("--player-space")).toBe("72px");
    unmount();
    expect(column.style.getPropertyValue("--player-space")).toBe("");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("a rejected listener reports an error and the other listener is cleaned up", async () => {
  const onError = vi.fn();
  const stopQueue = vi.fn().mockRejectedValue(new Error("unlisten failed"));
  vi.spyOn(events.playerState, "listen").mockRejectedValue(new Error("listen failed"));
  vi.spyOn(events.playerQueue, "listen").mockResolvedValue(stopQueue);
  const playerSnapshot = vi.fn(() => idleSnapshot);
  mockCommands({ playerSnapshot });
  const { unmount } = renderHook(() => usePlayerSubscription(onError));

  await vi.waitFor(() => expect(onError).toHaveBeenCalledExactlyOnceWith(
    "Could not listen for player updates: listen failed",
  ));
  expect(playerSnapshot).not.toHaveBeenCalled();
  unmount();
  await vi.waitFor(() => expect(stopQueue).toHaveBeenCalledOnce());
});

test.each(["pending seek", "unfinished drag"])("a track change clears a %s", async (seek) => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  if (seek === "pending seek") {
    fireEvent.pointerUp(slider);
    expect(handlers.seekTo).toHaveBeenCalledWith({ position: 120 });
  }
  await act(() => events.playerState.emit({
    state: playingState({ track: { ...testSong, video_id: "test-vid-02" }, position: 0 }),
  }));
  expect(screen.getByRole("slider", { name: "Seek" })).toHaveValue("0");
});

test("Escape dismisses the queue and returns focus to Queue", async () => {
  renderPlaying();
  const button = await screen.findByRole("button", { name: /Queue/ });
  await userEvent.click(button);
  await userEvent.tab({ shift: true });
  expect(button).not.toHaveFocus();

  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("region", { name: "Queue" })).not.toBeInTheDocument();
  expect(button).toHaveFocus();
});

test("a pointer press outside the player dismisses the queue", async () => {
  renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: /Queue/ }));
  const panel = screen.getByRole("region", { name: "Queue" });
  fireEvent.pointerDown(panel);
  expect(panel).toBeInTheDocument();
  fireEvent.pointerDown(document.body);
  expect(panel).not.toBeInTheDocument();
});

test("queue exposes its position and lets the user edit upcoming songs", async () => {
  const jumpTo = vi.fn(() => null);
  const removeQueueItem = vi.fn(() => null);
  const moveQueueItem = vi.fn(() => null);
  const clearUpcoming = vi.fn(() => null);
  mockCommands({
    playerSnapshot: () => ({
      state: playingState(),
      queue: { songs: [testSong, { ...testSong, title: "Second" }, { ...testSong, title: "Third" }], current: 0, browse_track: null },
    }),
    jumpTo, removeQueueItem, moveQueueItem, clearUpcoming,
  });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  const queue = await screen.findByRole("button", { name: "Queue", description: "1 of 3" });
  await userEvent.click(queue);
  const panel = within(screen.getByRole("region", { name: "Queue" }));
  const third = panel.getByRole("button", { name: "Third" });
  await userEvent.click(third);
  expect(jumpTo).toHaveBeenCalledWith({ index: 2 });
  await userEvent.keyboard("{Alt>}{ArrowUp}{/Alt}");
  expect(moveQueueItem).toHaveBeenCalledWith({ from: 2, to: 1 });
  const [, secondRow, thirdRow] = panel.getAllByRole("listitem");
  const dataTransfer = { setData: vi.fn(), dropEffect: "none" };
  fireEvent.dragStart(secondRow, { dataTransfer });
  fireEvent.dragOver(thirdRow, { dataTransfer });
  fireEvent.drop(thirdRow, { dataTransfer });
  expect(dataTransfer.dropEffect).toBe("move");
  expect(moveQueueItem).toHaveBeenLastCalledWith({ from: 1, to: 2 });
  await userEvent.click(panel.getByRole("button", { name: "Remove Second" }));
  expect(removeQueueItem).toHaveBeenCalledWith({ index: 1 });
  await userEvent.click(panel.getByRole("button", { name: "Clear upcoming" }));
  expect(clearUpcoming).toHaveBeenCalledTimes(1);

  await userEvent.click(panel.getByRole("button", { name: "More actions for Third" }));
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Queue" })).toBeInTheDocument();
});

test("rolls back a rejected rating after optimistically pressing Like", async () => {
  let reject!: (error: Error) => void;
  const rateSong = vi.fn(() => new Promise<null>((_, fail) => { reject = fail; }));
  mockCommands({
    playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }),
    songMetadata: () => ({ like_status: "INDIFFERENT", in_library: false, can_add: true, can_remove: false }),
    rateSong,
  });
  renderWithProviders(<PlayerProvider><PlayerBar /></PlayerProvider>);
  const like = await screen.findByRole("button", { name: "Like" });
  await waitFor(() => expect(like).toBeEnabled());
  await userEvent.click(like);
  expect(like).toHaveAttribute("aria-pressed", "true");
  expect(rateSong).toHaveBeenCalledWith({ videoId: testSong.video_id, rating: "LIKE" });
  await act(() => reject(new Error("Rating failed")));
  expect(like).toHaveAttribute("aria-pressed", "false");
  expect(await screen.findByRole("alert")).toHaveTextContent("Rating failed");
});
