import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands, type BrowsePage } from "../bindings";
import { idleSnapshot, playingState } from "../player/testing";
import { account, home, searchResults } from "../test/fixtures";
import { renderApp } from "../test/render";

const page: BrowsePage = {
  title: "Test release",
  subtitle: "Test artist · 2024",
  second_subtitle: "2 songs",
  thumbnail_url: "https://lh3.googleusercontent.com/test-release",
  songs: [
    { video_id: "test-vid-01", title: "Opening track", artists: [{ name: "Track artist", browse_id: null }], thumbnail_url: null },
    { video_id: "test-vid-02", title: "Final track", artists: [], thumbnail_url: null },
  ],
  continuation: null,
};

function renderDetailApp(handlers: Parameters<typeof mockCommands>[0] = {}) {
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => account,
    getBrowsePage: () => page,
    playerSnapshot: () => idleSnapshot,
    ...handlers,
  });
  renderApp();
}

async function openAlbum() {
  await userEvent.click(await screen.findByRole("button", { name: "Open Test album" }));
}

test("opens a Home album, displays its tracks, and queues playback from the clicked row", async () => {
  const getBrowsePage = vi.fn(() => page);
  const playQueue = vi.fn(() => null);
  renderDetailApp({
    getBrowsePage,
    playQueue,
    playerSnapshot: () => ({ ...idleSnapshot, state: playingState({ track: page.songs[0] }) }),
  });

  await openAlbum();
  expect(await screen.findByRole("heading", { name: page.title, level: 1 })).toBeInTheDocument();
  expect(getBrowsePage).toHaveBeenCalledWith({ browseId: "test-album" });
  expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
  expect(screen.getByText("Test artist · 2024")).toBeInTheDocument();
  expect(screen.getByText("2 songs")).toBeInTheDocument();
  const tracks = screen.getByRole("list", { name: "Tracks" });
  expect(within(tracks).getByRole("button", { name: /Opening track/ })).toHaveAttribute("aria-current", "true");
  await userEvent.click(within(tracks).getByRole("button", { name: /Final track/ }));
  expect(playQueue).toHaveBeenCalledWith({ songs: page.songs, startIndex: 1 });

  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByRole("heading", { name: "Home", level: 1 })).toBeInTheDocument();
});

test("returns to the filtered Search results with Escape or the Back shortcut", async () => {
  const search = vi.fn(({ filter }: { filter: string | null }) => filter === "albums"
    ? { items: searchResults.items.filter((item) => item.kind === "album"), continuation: null }
    : searchResults);
  renderDetailApp({ search, searchSuggestions: () => [] });

  await userEvent.click(await screen.findByRole("button", { name: "Search" }));
  await userEvent.type(screen.getByRole("combobox"), "test search{Enter}");
  await screen.findByRole("region", { name: "Top result" });
  await userEvent.click(await screen.findByRole("button", { name: "Albums" }));
  for (const shortcut of [{ key: "Escape" }, { key: "[", ctrlKey: true }]) {
    await userEvent.click(await screen.findByRole("button", { name: "Open Found album" }));
    expect(await screen.findByRole("heading", { name: page.title })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Search" })).toHaveAttribute("aria-current", "page");
    fireEvent.keyDown(screen.getByRole("button", { name: "Back" }), shortcut);
    expect(screen.getByRole("combobox")).toHaveValue("test search");
    expect(screen.getByRole("button", { name: "Albums" })).toHaveAttribute("aria-pressed", "true");
  }
  expect(search.mock.calls).toEqual([
    [{ query: "test search", filter: null }],
    [{ query: "test search", filter: "albums" }],
  ]);
});

test("opens a playlist from the top search result", async () => {
  const getBrowsePage = vi.fn(() => page);
  renderDetailApp({
    getBrowsePage,
    searchSuggestions: () => [],
    search: () => ({
      items: [{ kind: "playlist", browse_id: "VLtest", title: "Found playlist", subtitle: null, thumbnail_url: null }],
      continuation: null,
    }),
  });

  await userEvent.click(await screen.findByRole("button", { name: "Search" }));
  await userEvent.type(screen.getByRole("combobox"), "playlist{Enter}");
  const topResult = await screen.findByRole("region", { name: "Top result" });
  await userEvent.click(within(topResult).getByRole("button", { name: "Open Found playlist" }));
  expect(await screen.findByRole("heading", { name: page.title })).toBeInTheDocument();
  expect(getBrowsePage).toHaveBeenCalledWith({ browseId: "VLtest" });
});

test("shows a loading skeleton, then Retry and an empty state", async () => {
  let rejectLoad: (reason: unknown) => void = () => {};
  const getBrowsePage = vi.fn<() => BrowsePage | Promise<BrowsePage>>()
    .mockImplementationOnce(() => new Promise((_, reject) => { rejectLoad = reject; }))
    .mockReturnValue({ ...page, songs: [] });
  renderDetailApp({ getBrowsePage });

  await openAlbum();
  expect(screen.getByRole("status")).toHaveTextContent("Loading");
  rejectLoad({ kind: "parse", message: "Could not read page" });
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not read page");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("No tracks to play.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Play" })).toBeDisabled();
});

test("Play caps the queue and Shuffle enables shuffle before playing", async () => {
  const songs = Array.from({ length: 501 }, (_, index) => ({ ...page.songs[0], title: `Track ${index + 1}` }));
  const setShuffle = vi.fn(() => null);
  const playQueue = vi.fn(() => null);
  renderDetailApp({ getBrowsePage: () => ({ ...page, songs }), setShuffle, playQueue });

  await openAlbum();
  await userEvent.click(await screen.findByRole("button", { name: "Play" }));
  expect(playQueue).toHaveBeenLastCalledWith({ songs: songs.slice(0, 500), startIndex: 0 });
  await userEvent.click(screen.getByRole("button", { name: "Shuffle" }));
  expect(setShuffle).toHaveBeenCalledWith({ enabled: true });
  expect(setShuffle.mock.invocationCallOrder[0]).toBeLessThan(playQueue.mock.invocationCallOrder[1]);
  expect(playQueue).toHaveBeenLastCalledWith({ songs: songs.slice(0, 500), startIndex: expect.any(Number) });
  await userEvent.click(screen.getByRole("button", { name: /Track 501/ }));
  expect(playQueue).toHaveBeenLastCalledWith({ songs: songs.slice(500), startIndex: 0 });
});
