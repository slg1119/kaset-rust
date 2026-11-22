import { within, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands, type ArtistPage, type Song } from "../bindings";
import { idleSnapshot } from "../player/testing";
import { home, searchResults } from "../test/fixtures";
import { renderApp } from "../test/render";

const song: Song = {
  video_id: "test-vid-11",
  title: "Found song",
  artists: [
    { name: "Linked artist", browse_id: "UC-linked" },
    { name: "Plain credit", browse_id: null },
  ],
  thumbnail_url: null,
};

const artist: ArtistPage = {
  name: "Test Artist",
  description: "Test biography",
  thumbnail_url: "https://lh3.googleusercontent.com/test-artist",
  subscribers: "123 subscribers",
  channel_id: "UCtest-artist",
  subscribed: false,
  radio_playlist_id: null,
  radio_video_id: null,
  radio_params: null,
  shuffle_playlist_id: null,
  shuffle_video_id: null,
  shuffle_params: null,
  top_songs_section: 0,
  sections: [{
    title: "Top songs",
    items: [{ kind: "song", ...song }],
    more: { browse_id: "test-songs", params: "test%3D" },
  }],
};

function mount(page = artist, track: Song | null = null, overrides?: Parameters<typeof mockCommands>[0]) {
  const getArtist = vi.fn(() => page);
  const getArtistMore = vi.fn(() => [{ kind: "song" as const, ...song }]);
  const playQueue = vi.fn(() => null);
  const playArtistMix = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getAccount: () => null,
    windowVibrancy: () => false,
    playerSnapshot: () => ({ ...idleSnapshot, state: { ...idleSnapshot.state, track } }),
    getFeed: () => home,
    search: () => searchResults,
    searchSuggestions: () => [],
    getArtist, getArtistMore, playQueue, playArtistMix,
    ...overrides,
  });
  renderApp();
  return { getArtist, getArtistMore, playQueue, playArtistMix };
}

test("opens a Home artist, plays top songs, and navigates to See all and back", async () => {
  const { getArtist, getArtistMore, playQueue } = mount();
  await userEvent.click(await screen.findByRole("button", { name: "Open Round artist" }));
  expect(await screen.findByRole("heading", { name: "Test Artist", level: 1 })).toBeInTheDocument();
  expect(getArtist).toHaveBeenCalledWith({ browseId: "UCtest-artist" });
  expect(screen.getByText("123 subscribers")).toBeInTheDocument();
  expect(screen.getByText("Test biography")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Play" }));
  expect(playQueue).toHaveBeenCalledWith({ songs: [song], startIndex: 0 });

  await userEvent.click(screen.getByRole("button", { name: "See all Top songs" }));
  expect(await screen.findByRole("heading", { name: "Top songs", level: 1 })).toBeInTheDocument();
  expect(await screen.findByRole("button", { name: /Play Found song/ })).toBeInTheDocument();
  expect(getArtistMore).toHaveBeenCalledWith({ browseId: "test-songs", params: "test%3D" });
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByRole("heading", { name: "Test Artist", level: 1 })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByRole("heading", { name: "Home", level: 1 })).toBeInTheDocument();
});

test("plays the server artist mix and opens linked track and player credits", async () => {
  const { getArtist, playArtistMix, playQueue } = mount({
    ...artist, radio_playlist_id: "RD-test", radio_params: "test%3D",
  }, song);
  await userEvent.click(await screen.findByRole("button", { name: "Open Round artist" }));
  await screen.findByRole("heading", { name: "Test Artist" });
  await userEvent.click(within(screen.getByRole("main")).getByRole("button", { name: "Play" }));
  expect(playArtistMix).toHaveBeenCalledWith({ playlistId: "RD-test", videoId: null, params: "test%3D" });

  const shelf = screen.getByRole("region", { name: "Top songs" });
  expect(within(shelf).getByText(/Plain credit/)).toBeInTheDocument();
  expect(within(shelf).queryByRole("button", { name: "Open Plain credit" })).not.toBeInTheDocument();
  await userEvent.click(within(shelf).getByRole("button", { name: "Open Linked artist" }));
  await vi.waitFor(() => expect(getArtist).toHaveBeenLastCalledWith({ browseId: "UC-linked" }));
  expect(playQueue).not.toHaveBeenCalled();

  await userEvent.click(screen.getByRole("button", { name: "Home" }));
  await userEvent.click(within(screen.getByRole("contentinfo")).getByRole("button", { name: "Open Linked artist" }));
  expect(await screen.findByRole("heading", { name: "Test Artist", level: 1 })).toBeInTheDocument();
});

test("opens an artist from Search and restores the result on back", async () => {
  mount();
  await userEvent.click(await screen.findByRole("button", { name: "Search" }));
  const input = screen.getByRole("combobox");
  await userEvent.type(input, "test{Enter}");
  const top = await screen.findByRole("region", { name: "Top result" });
  await userEvent.click(within(top).getByRole("button", { name: "Open Test artist" }));
  expect(await screen.findByRole("heading", { name: "Test Artist", level: 1 })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(within(await screen.findByRole("region", { name: "Top result" })).getByRole("button", { name: "Open Test artist" })).toBeInTheDocument();
});


test("retries artist load in place and reports a shuffle action failure", async () => {
  const getArtist = vi.fn()
    .mockRejectedValueOnce({ kind: "network", message: "Artist could not load." })
    .mockResolvedValue({
      ...artist, shuffle_playlist_id: "RDAO-test", shuffle_params: "test%3D",
    });
  const playArtistMix = vi.fn().mockRejectedValue({ kind: "network", message: "Could not start mix." });
  mount(artist, null, { getArtist, playArtistMix });
  await userEvent.click(await screen.findByRole("button", { name: "Explore" }));
  await userEvent.click(await screen.findByRole("button", { name: "Open Round artist" }));
  const main = within(screen.getByRole("main"));
  expect(await main.findByRole("alert")).toHaveTextContent("Artist could not load.");
  await userEvent.click(main.getByRole("button", { name: "Retry" }));
  await main.findByRole("heading", { name: "Test Artist" });
  await userEvent.click(main.getByRole("button", { name: "Play" }));
  expect(playArtistMix).toHaveBeenCalledWith({ playlistId: "RDAO-test", videoId: null, params: "test%3D" });
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not start mix.");
  expect(main.queryByRole("alert")).not.toBeInTheDocument();
});
