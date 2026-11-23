import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands, type Episode, type PodcastShow } from "../bindings";
import { idleSnapshot } from "../player/testing";
import { account, home } from "../test/fixtures";
import { renderApp } from "../test/render";

const podcast = {
  kind: "podcast" as const,
  browse_id: "MPSPPtest",
  title: "Test podcast",
  subtitle: "Test host",
  thumbnail_url: null,
};
const episode: Episode = {
  video_id: "test-vid-11",
  title: "First episode",
  description: "About the episode",
  date: "Nov 22, 2026",
  duration: "42 min",
  thumbnail_url: null,
  progress: 0.25,
  show_title: null,
  show_browse_id: null,
};
const secondEpisode = { ...episode, video_id: "test-vid-12", title: "Second episode" };
const feed = { ...home, sections: [{ title: "Recommended shows", items: [podcast] }] };
const show: PodcastShow = {
  title: podcast.title,
  author: podcast.subtitle,
  description: "About the show",
  thumbnail_url: "https://i.ytimg.com/test-show",
  episodes: [episode],
  continuation: "test-continuation",
};

function renderPodcasts(handlers: Parameters<typeof mockCommands>[0] = {}) {
  mockCommands({
    authStatus: () => "signed_in",
    getAccount: () => account,
    getFeed: () => feed,
    getPodcasts: () => feed,
    getPodcastShow: () => show,
    getPodcastEpisodesContinuation: () => ({ episodes: [secondEpisode], continuation: null }),
    playerSnapshot: () => idleSnapshot,
    searchSuggestions: () => [],
    search: () => ({ items: [podcast], continuation: null }),
    ...handlers,
  });
  renderApp();
}

async function openShow() {
  await userEvent.click(await screen.findByRole("button", { name: "Open Test podcast, Test host" }));
  expect(await screen.findByRole("heading", { name: show.title, level: 1 })).toBeInTheDocument();
}

test("opens the podcast feed and show, loads more episodes, and queues from the selected row", async () => {
  const getPodcasts = vi.fn(() => feed);
  const getPodcastShow = vi.fn(() => show);
  const getPodcastEpisodesContinuation = vi.fn(() => ({
    episodes: [episode, secondEpisode],
    continuation: "test-continuation",
  }));
  const playQueue = vi.fn(() => null);
  renderPodcasts({ getPodcasts, getPodcastShow, getPodcastEpisodesContinuation, playQueue });

  const nav = within(await screen.findByRole("navigation", { name: "Main" }));
  await userEvent.click(nav.getByRole("button", { name: "Podcasts" }));
  expect(await screen.findByRole("heading", { name: "Podcasts", level: 1 })).toBeInTheDocument();
  await openShow();
  expect(getPodcasts).toHaveBeenCalledTimes(1);
  expect(getPodcastShow).toHaveBeenCalledWith({ browseId: "MPSPPtest" });
  expect(nav.getByRole("button", { name: "Podcasts" })).toHaveAttribute("aria-current", "page");
  expect(screen.getByText("About the show")).toBeInTheDocument();
  // The covered feed stays mounted (inert) under the show.
  expect(within(screen.getByRole("main")).getByText("Test host")).toBeInTheDocument();
  expect(screen.getByText(/Nov 22, 2026.*42 min/)).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Load more episodes" }));
  const episodes = within(screen.getByRole("list", { name: "Episodes" }));
  await userEvent.click(await episodes.findByRole("button", { name: /Play Second episode/ }));
  expect(episodes.getAllByRole("listitem")).toHaveLength(2);
  expect(screen.queryByRole("button", { name: "Load more episodes" })).not.toBeInTheDocument();
  expect(getPodcastEpisodesContinuation).toHaveBeenCalledWith({ token: "test-continuation" });
  expect(playQueue).toHaveBeenCalledWith({
    songs: [episode, secondEpisode].map(({ video_id, title }) => ({
      video_id, title, thumbnail_url: show.thumbnail_url, is_podcast_episode: true,
      artists: [{ name: show.title, browse_id: null }],
    })),
    startIndex: 1,
  });

  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByRole("heading", { name: "Podcasts", level: 1 })).toBeInTheDocument();
});

test("opens the same show from Home and Search", async () => {
  const getBrowsePage = vi.fn();
  renderPodcasts({ getBrowsePage });
  await openShow();
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  const nav = within(screen.getByRole("navigation", { name: "Main" }));
  await userEvent.click(nav.getByRole("button", { name: "Search" }));
  await userEvent.type(screen.getByRole("combobox"), "test podcast{Enter}");
  await openShow();
  expect(getBrowsePage).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(screen.getByRole("combobox")).toHaveValue("test podcast");
});

test("offers in-screen retries for show and continuation failures", async () => {
  const getPodcastShow = vi.fn<() => PodcastShow>()
    .mockImplementationOnce(() => { throw { kind: "parse", message: "Could not load show" }; })
    .mockReturnValue(show);
  const getPodcastEpisodesContinuation = vi.fn()
    .mockImplementationOnce(() => { throw { kind: "parse", message: "Could not load episodes" }; })
    .mockReturnValue({ episodes: [], continuation: null });
  renderPodcasts({ getPodcastShow, getPodcastEpisodesContinuation });
  await userEvent.click(await screen.findByRole("button", { name: "Open Test podcast, Test host" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not load show");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("heading", { name: show.title, level: 1 })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Load more episodes" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not load episodes");
  expect(screen.getByRole("button", { name: /Play First episode/ })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Retry loading episodes" }));
  expect(getPodcastEpisodesContinuation).toHaveBeenCalledTimes(2);
});

test("keeps episodes playable and uses the route title when the show title is missing", async () => {
  const playQueue = vi.fn(() => null);
  renderPodcasts({ getPodcastShow: () => ({ ...show, title: "" }), playQueue });
  await openShow();
  await userEvent.click(screen.getByRole("button", { name: "Play First episode" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [expect.objectContaining({ artists: [{ name: podcast.title, browse_id: null }] })],
    startIndex: 0,
  });
});

test("hides regional Podcasts for the session and resets on account replacement and sign-out", async () => {
  const getPodcasts = vi.fn()
    .mockImplementationOnce(() => { throw { kind: "podcasts_unavailable", message: "HTTP 404" }; })
    .mockImplementationOnce(() => { throw { kind: "podcasts_unavailable", message: "HTTP 404" }; })
    .mockReturnValue(feed);
  renderPodcasts({ getPodcasts });
  const nav = () => within(screen.getByRole("navigation", { name: "Main" }));
  const expectUnavailable = async () => {
    await userEvent.click(await screen.findByRole("button", { name: "Podcasts" }));
    expect(await screen.findByText("Podcasts aren't available in your region.")).toBeInTheDocument();
    expect(nav().queryByRole("button", { name: "Podcasts" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  };

  await expectUnavailable();
  await userEvent.click(nav().getByRole("button", { name: "Home" }));
  expect(nav().queryByRole("button", { name: "Podcasts" })).not.toBeInTheDocument();
  await act(() => events.authChanged.emit({ status: "signed_in" }));
  await expectUnavailable();
  await act(() => events.authChanged.emit({ status: "signed_out" }));
  await act(() => events.authChanged.emit({ status: "signed_in" }));
  await userEvent.click(await screen.findByRole("button", { name: "Podcasts" }));
  expect(await screen.findByRole("heading", { name: "Recommended shows" })).toBeInTheDocument();
  expect(getPodcasts).toHaveBeenCalledTimes(3);
});

test("keeps Podcasts visible and offers Retry on other feed errors", async () => {
  const getPodcasts = vi.fn()
    .mockImplementationOnce(() => { throw { kind: "http", message: "HTTP 500" }; })
    .mockReturnValue(feed);
  renderPodcasts({ getPodcasts });
  await userEvent.click(await screen.findByRole("button", { name: "Podcasts" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("HTTP 500");
  expect(screen.getByRole("button", { name: "Podcasts" })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("heading", { name: "Recommended shows" })).toBeInTheDocument();
});
