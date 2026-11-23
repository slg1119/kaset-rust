import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands, type LibraryKind, type ShelfItem } from "../bindings";
import { idleSnapshot, testSong } from "../player/testing";
import { home } from "../test/fixtures";
import { renderApp } from "../test/render";

function mount(handlers: Parameters<typeof mockCommands>[0]) {
  mockCommands({
    authStatus: () => "signed_in",
    getAccount: () => null,
    getFeed: () => home,
    playerSnapshot: () => idleSnapshot,
    ...handlers,
  });
  renderApp();
}

test("requests each collection from the Library group and renders its cards in the feed grid", async () => {
  const items = {
    playlists: { kind: "playlist", title: "Saved playlist", browse_id: "VLtest" },
    albums: { kind: "album", title: "Saved album", browse_id: "MPREtest" },
    artists: { kind: "artist", title: "Followed artist", browse_id: "UCtest" },
  } as const;
  const getLibrary = vi.fn(({ kind }: { kind: LibraryKind }) => {
    if (kind === "songs") throw new Error("Unexpected collection");
    const item: ShelfItem = { ...items[kind], subtitle: "Test metadata", thumbnail_url: null };
    return { kind: "collection" as const, data: [{ title: "Saved", items: [item] }] };
  });
  mount({ getLibrary });
  const library = within(await screen.findByRole("region", { name: "Library" }));

  for (const [kind, label] of [["playlists", "Playlists"], ["albums", "Albums"], ["artists", "Artists"]] as const) {
    const tab = library.getByRole("button", { name: label });
    await userEvent.click(tab);
    expect(tab).toHaveAttribute("aria-current", "page");
    const grid = within(screen.getByRole("main"));
    const card = await grid.findByRole("button", { name: `Open ${items[kind].title}, Test metadata` });
    expect(card.closest("ul")).toHaveClass("artist-items-grid");
    expect(getLibrary).toHaveBeenLastCalledWith({ kind });
  }
});

test("opens Liked songs through the shared detail screen and pages its tracks", async () => {
  const getLibrary = vi.fn(() => ({
    kind: "songs" as const,
    data: {
      title: "Liked songs", subtitle: null, second_subtitle: null,
      thumbnail_url: null, songs: [testSong], continuation: "test-liked-next",
    },
  }));
  const getBrowsePageContinuation = vi.fn(() => ({
    songs: [{ ...testSong, title: "Another liked song" }], continuation: null,
  }));
  mount({ getLibrary, getBrowsePageContinuation });

  await userEvent.click(await screen.findByRole("button", { name: "Liked songs" }));
  expect(await screen.findByRole("heading", { name: "Liked songs" })).toBeInTheDocument();
  expect(getLibrary).toHaveBeenCalledWith({ kind: "songs" });
  await userEvent.click(screen.getByRole("button", { name: "Load more" }));
  expect(getBrowsePageContinuation).toHaveBeenCalledWith({ token: "test-liked-next" });
  expect(await screen.findByRole("button", { name: "Another liked song, Test artist" })).toBeInTheDocument();
});
