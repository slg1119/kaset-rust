import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands, type BrowsePage, type BrowsePageContinuation } from "../bindings";
import { idleSnapshot } from "../player/testing";
import { account, home, searchResults } from "../test/fixtures";
import { renderApp } from "../test/render";

const cover = "https://lh3.googleusercontent.com/test-release";

const page: BrowsePage = {
  title: "Test release",
  subtitle: "Test artist · 2024",
  second_subtitle: "2 songs",
  thumbnail_url: cover,
  songs: [
    { video_id: "test-vid-01", title: "Opening track", artists: [{ name: "Track artist", browse_id: null }], thumbnail_url: cover },
    { video_id: "test-vid-02", title: "Final track", artists: [], thumbnail_url: cover },
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
  await userEvent.click(await screen.findByRole("button", { name: "Open Test album, Album • 2024" }));
}

test("opens a Home album, displays its tracks, and queues playback from the clicked row", async () => {
  const getBrowsePage = vi.fn(() => page);
  const playerSnapshot = vi.fn(() => idleSnapshot);
  const playBrowseQueue = vi.fn(() => null);
  renderDetailApp({
    getBrowsePage,
    playBrowseQueue,
    playerSnapshot,
  });

  await screen.findByRole("heading", { name: "Home", level: 1 });
  const homeFrame = screen.getByRole("main");
  homeFrame.scrollTop = 240;
  await openAlbum();
  expect(await screen.findByRole("heading", { name: page.title, level: 1 })).toBeInTheDocument();
  expect(getBrowsePage).toHaveBeenCalledWith({ browseId: "test-album" });
  expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
  expect(screen.getByText("Test artist · 2024")).toBeInTheDocument();
  expect(screen.getByText("2 songs")).toBeInTheDocument();
  const tracks = screen.getByRole("list", { name: "Tracks" });
  expect(playerSnapshot).toHaveBeenCalledTimes(1);
  await userEvent.click(within(tracks).getByRole("button", { name: /Final track/ }));
  expect(playBrowseQueue).toHaveBeenCalledWith({ songs: page.songs, startIndex: 1, playback: { browse_id: "test-album", offset: 0, shuffle: false } });

  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByRole("heading", { name: "Home", level: 1 })).toBeInTheDocument();
  expect(screen.getByRole("main")).toBe(homeFrame);
  expect(homeFrame.scrollTop).toBe(240);
  expect(screen.getByRole("button", { name: /Open Test album/ })).toHaveFocus();
});

test("returns to the filtered Search results with Escape or the Back shortcut", async () => {
  const search = vi.fn(() => searchResults);
  renderDetailApp({ search, searchSuggestions: () => [] });

  await userEvent.click(await screen.findByRole("button", { name: "Search" }));
  await userEvent.type(screen.getByRole("combobox"), "test search{Enter}");
  await userEvent.click(await within(screen.getByRole("main")).findByRole("button", { name: "Albums" }));
  for (const shortcut of [{ key: "Escape" }, { key: "[", ctrlKey: true }]) {
    await userEvent.click(await screen.findByRole("button", { name: "Open Found album, Test artist • 2024" }));
    expect(await screen.findByRole("heading", { name: page.title })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Search" })).toHaveAttribute("aria-current", "page");
    fireEvent.keyDown(screen.getByRole("button", { name: "Back" }), shortcut);
    expect(screen.getByRole("combobox")).toHaveValue("test search");
    expect(screen.getByRole("button", { name: /Open Found album/ })).toHaveFocus();
    expect(within(screen.getByRole("main")).getByRole("button", { name: "Albums" })).toHaveAttribute("aria-pressed", "true");
  }
  expect(search).toHaveBeenCalledTimes(1);
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

test("caps Play and Shuffle at 500 and can start at a later row", async () => {
  const songs = Array.from({ length: 501 }, (_, index) => ({ ...page.songs[0], title: `Track ${index + 1}` }));
  const playBrowseQueue = vi.fn(() => null);
  const getBrowsePageContinuation = vi.fn(() => ({ songs: songs.slice(499), continuation: "test-more" }));
  renderDetailApp({
    getBrowsePage: () => ({ ...page, songs: songs.slice(0, 499), continuation: "test-next" }),
    getBrowsePageContinuation,
    playBrowseQueue,
  });

  await openAlbum();
  const main = within(screen.getByRole("main"));
  await userEvent.click(await main.findByRole("button", { name: "Play" }));
  await waitFor(() => expect(playBrowseQueue).toHaveBeenLastCalledWith({
    songs: songs.slice(0, 500), startIndex: 0, playback: { browse_id: "test-album", offset: 0, shuffle: false },
  }));
  expect(getBrowsePageContinuation).toHaveBeenCalledExactlyOnceWith({ token: "test-next" });
  await userEvent.click(main.getByRole("button", { name: "Shuffle" }));
  expect(playBrowseQueue).toHaveBeenLastCalledWith({
    songs: songs.slice(0, 500), startIndex: expect.any(Number), playback: { browse_id: "test-album", offset: 0, shuffle: true },
  });
  await userEvent.click(main.getByRole("button", { name: /Track 501/ }));
  expect(playBrowseQueue).toHaveBeenLastCalledWith({
    songs: songs.slice(500), startIndex: 0, playback: { browse_id: "test-album", offset: 500, shuffle: false },
  });
});

test.each(["Play", "Shuffle"])("%s waits for continuation pages and passes the combined tracks", async (action) => {
  let resolveNext!: (value: BrowsePageContinuation) => void;
  const nextSong = { ...page.songs[0], title: "From the next page" };
  const getBrowsePageContinuation = vi.fn()
    .mockImplementationOnce(() => new Promise((resolve) => { resolveNext = resolve; }))
    .mockResolvedValue({ songs: [nextSong], continuation: "test-another-page" });
  const playBrowseQueue = vi.fn(() => null);
  renderDetailApp({
    getBrowsePage: () => ({ ...page, continuation: "test-continuation" }),
    getBrowsePageContinuation,
    playBrowseQueue,
  });

  await openAlbum();
  await userEvent.click(await screen.findByRole("button", { name: action }));
  expect(getBrowsePageContinuation).toHaveBeenCalledWith({ token: "test-continuation" });
  expect(playBrowseQueue).not.toHaveBeenCalled();
  resolveNext({ songs: [nextSong], continuation: "test-another-page" });
  await waitFor(() => expect(playBrowseQueue).toHaveBeenCalledWith({
    songs: [...page.songs, nextSong, nextSong],
    startIndex: action === "Shuffle" ? expect.any(Number) : 0,
    playback: { browse_id: "test-album", offset: 0, shuffle: action === "Shuffle" },
  }));
  expect(getBrowsePageContinuation).toHaveBeenCalledTimes(2);
});

test("scroll observation loads the next page and a failure can be retried", async () => {
  let notify: IntersectionObserverCallback | undefined;
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: IntersectionObserverCallback) { notify = callback; }
    observe() {}
    disconnect() {}
  });
  try {
    const nextSong = { ...page.songs[0], title: "From the next page" };
    const getBrowsePageContinuation = vi.fn()
      .mockRejectedValueOnce({ kind: "network", message: "Could not load more tracks." })
      .mockResolvedValue({ songs: [nextSong], continuation: null });
    renderDetailApp({
      getBrowsePage: () => ({ ...page, continuation: "test-continuation" }),
      getBrowsePageContinuation,
    });
    await openAlbum();
    await screen.findByRole("button", { name: "Load more" });
    act(() => notify?.([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load more tracks.");
    await userEvent.click(screen.getByRole("button", { name: "Retry loading more" }));
    expect(await screen.findByRole("button", { name: /From the next page/ })).toBeInTheDocument();
    expect(getBrowsePageContinuation).toHaveBeenCalledTimes(2);
  } finally {
    vi.unstubAllGlobals();
  }
});

test("marks only the playing occurrence of a repeated track", async () => {
  const duplicate = page.songs[0];
  const songs = [duplicate, duplicate];
  renderDetailApp({ getBrowsePage: () => ({ ...page, songs }) });
  await openAlbum();
  const tracks = await screen.findByRole("list", { name: "Tracks" });
  await act(() => events.playerQueue.emit({
    queue: { songs, current: 0, browse_track: { browse_id: "test-album", index: 1 } },
  }));
  const rows = within(tracks).getAllByRole("button");
  expect(rows[0]).not.toHaveAttribute("aria-current");
  expect(rows[1]).toHaveAttribute("aria-current", "true");
});
