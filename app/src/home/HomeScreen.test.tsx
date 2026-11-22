import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { home } from "../test/fixtures";
import { renderWithProviders } from "../test/render";
import { HomeScreen } from "./HomeScreen";

test("the first all-song shelf is a song grid and the rest are carousels", async () => {
  mockCommands({ getHome: () => home });
  const { container } = renderWithProviders(<HomeScreen />);

  const quickPicks = await screen.findByRole("region", { name: "Quick picks" });
  expect(quickPicks.querySelector(".song-grid")).not.toBeNull();
  expect(within(quickPicks).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
    "Play First song, Test artist",
    "Play Second song",
  ]);

  const mixed = screen.getByRole("region", { name: "Mixed for you" });
  expect(mixed.querySelector(".carousel")).not.toBeNull();
  expect(within(mixed).getByText("Album • 2024")).toBeInTheDocument();
  // Only songs are interactive until album, artist, and playlist pages exist (M5).
  expect(within(mixed).getByRole("button", { name: "Play Mixed song" })).toBeInTheDocument();
  expect(within(mixed).queryByRole("button", { name: /Test album|Round artist/ })).not.toBeInTheDocument();
  expect(within(mixed).getByRole("button", { name: "Next in Mixed for you" })).toBeInTheDocument();

  const images = [...container.querySelectorAll("img")];
  expect(images.map((img) => img.getAttribute("src"))).toEqual([
    "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
    "https://lh3.googleusercontent.com/test-album=w226-h226",
    "https://yt3.ggpht.com/test-artist=s226",
  ]);
  expect(images[2]).toHaveClass("artwork-round");
  expect(images[1]).toHaveClass("artwork-square");
});

test("carousel buttons page by the visible width", async () => {
  const scrollBy = vi.fn();
  HTMLElement.prototype.scrollBy = scrollBy;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(600);
  try {
    mockCommands({ getHome: () => home });
    renderWithProviders(<HomeScreen />);
    await userEvent.click(await screen.findByRole("button", { name: "Next in Mixed for you" }));
    await userEvent.click(screen.getByRole("button", { name: "Previous in Mixed for you" }));
    expect(scrollBy.mock.calls).toEqual([
      [{ left: 600, behavior: "smooth" }],
      [{ left: -600, behavior: "smooth" }],
    ]);
  } finally {
    // @ts-expect-error -- jsdom has no scrollBy; remove the stub again.
    delete HTMLElement.prototype.scrollBy;
  }
});

test("clicking a song plays its shelf's songs from that song", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({ getHome: () => home, playQueue });
  renderWithProviders(<HomeScreen />);

  await userEvent.click(await screen.findByRole("button", { name: "Play Second song" }));
  expect(playQueue).toHaveBeenLastCalledWith({
    songs: [
      {
        video_id: "test-vid-01",
        title: "First song",
        artists: [{ name: "Test artist", browse_id: null }],
        thumbnail_url: "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
      },
      { video_id: "test-vid-02", title: "Second song", artists: [], thumbnail_url: null },
    ],
    startIndex: 1,
  });

  await userEvent.click(screen.getByRole("button", { name: "Play Mixed song" }));
  expect(playQueue).toHaveBeenLastCalledWith({
    songs: [{ video_id: "test-vid-03", title: "Mixed song", artists: [], thumbnail_url: null }],
    startIndex: 0,
  });
});

test("a failed play shows one notice", async () => {
  mockCommands({
    getHome: () => home,
    playQueue: () => {
      throw { kind: "not_signed_in", message: "Sign in to continue." };
    },
  });
  renderWithProviders(<HomeScreen />);
  await userEvent.click(await screen.findByRole("button", { name: "Play Second song" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Sign in to continue.");
  expect(screen.getAllByRole("alert")).toHaveLength(1);
});

test("load errors offer a retry", async () => {
  let calls = 0;
  mockCommands({
    getHome: () => {
      calls += 1;
      if (calls === 1) throw { kind: "network", message: "network error" };
      return home;
    },
  });
  renderWithProviders(<HomeScreen />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network error");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("region", { name: "Quick picks" })).toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("refresh refetches the feed", async () => {
  const getHome = vi.fn(() => home);
  mockCommands({ getHome });
  renderWithProviders(<HomeScreen />);
  await screen.findByRole("region", { name: "Quick picks" });
  await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await vi.waitFor(() => expect(getHome).toHaveBeenCalledTimes(2));
});

test("an empty feed says so", async () => {
  mockCommands({ getHome: () => ({ sections: [], continuation: null }) });
  renderWithProviders(<HomeScreen />);
  expect(await screen.findByText("Nothing to show yet.")).toBeInTheDocument();
});
