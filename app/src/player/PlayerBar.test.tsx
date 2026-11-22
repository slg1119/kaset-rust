import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "../bindings";
import { formatTime } from "./format";
import { PlayerBar } from "./PlayerBar";
import { idleSnapshot, playingState, testSong } from "./testing";

function renderPlaying(overrides: Parameters<typeof playingState>[0] = {}) {
  const handlers = {
    playerSnapshot: () => ({
      state: playingState(overrides),
      queue: { songs: [testSong, testSong], current: 0 },
    }),
    togglePlayback: vi.fn(() => null),
    nextTrack: vi.fn(() => null),
    previousTrack: vi.fn(() => null),
    seekTo: vi.fn(() => null),
    setRepeat: vi.fn(() => null),
    setShuffle: vi.fn(() => null),
  };
  mockCommands(handlers);
  render(<PlayerBar />);
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
  const { container } = render(<PlayerBar />);
  await vi.waitFor(() => expect(playerSnapshot).toHaveBeenCalled());
  expect(container).toBeEmptyDOMElement();
});

test("shows the track from player://state", async () => {
  mockCommands({ playerSnapshot: () => idleSnapshot });
  render(<PlayerBar />);
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
    events.playerQueue.emit({ queue: { songs: [testSong, testSong, testSong], current: 2 } }),
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

test("next is disabled at the end of the queue", async () => {
  renderPlaying({ has_next: false, status: "paused" });
  expect(await screen.findByRole("button", { name: "Next" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
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
  render(<PlayerBar />);
  await userEvent.click(await screen.findByRole("button", { name: "Next" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("player window failed");
});
