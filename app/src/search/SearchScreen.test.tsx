import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { searchResults } from "../test/fixtures";
import { renderWithProviders } from "../test/render";
import type { Filter } from "./groups";
import { SearchScreen } from "./SearchScreen";

/** SearchScreen with the query state the shell normally keeps. */
function Harness({ initial = "" }: { initial?: string }) {
  const [query, setQuery] = useState(initial);
  const [filter, setFilter] = useState<Filter>("all");
  return (
    <SearchScreen
      query={query}
      onSubmit={setQuery}
      focusRequest={0}
      filter={filter}
      onFilterChange={setFilter}
      onOpenPage={() => {}}
    />
  );
}

function field() {
  return screen.getByRole("combobox", { name: "Search YouTube Music" });
}

test("submitting shows the top result, then results grouped by type", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ search, searchSuggestions: () => [] });
  renderWithProviders(<Harness />);

  expect(field()).toHaveFocus();
  await userEvent.type(field(), "  test query {Enter}");
  expect(search).toHaveBeenCalledWith({ query: "test query" });

  const headings = await screen.findAllByRole("heading", { level: 2 });
  expect(headings.map((heading) => heading.textContent)).toEqual(["Top result", "Songs", "Videos", "Albums"]);
  const top = screen.getByRole("region", { name: "Top result" });
  expect(within(top).getByText("Test artist")).toBeInTheDocument();
  expect(within(top).getByText("Artist • 42K subscribers")).toBeInTheDocument();
  expect(within(top).queryByRole("button")).not.toBeInTheDocument();

  const songs = screen.getByRole("region", { name: "Songs" });
  expect(within(songs).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
    "Play Found song, Test artist",
    "Play Another song",
  ]);
  const albums = screen.getByRole("region", { name: "Albums" });
  expect(within(albums).getByRole("button", { name: "Open Found album" })).toBeInTheDocument();
  expect(within(albums).getByText("Test artist • 2024")).toBeInTheDocument();
});

test("filter chips narrow the loaded results without a new request", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ search, searchSuggestions: () => [] });
  renderWithProviders(<Harness initial="test query" />);

  const chips = await screen.findByRole("group", { name: "Filter results" });
  expect(within(chips).getAllByRole("button").map((chip) => chip.textContent)).toEqual([
    "All",
    "Songs",
    "Albums",
    "Artists",
    "Playlists",
  ]);
  expect(within(chips).getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "true");

  await userEvent.click(within(chips).getByRole("button", { name: "Albums" }));
  expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual(["Albums"]);
  await userEvent.click(within(chips).getByRole("button", { name: "Artists" }));
  expect(screen.getByRole("region", { name: "Artists" })).toHaveTextContent("Test artist");
  await userEvent.click(within(chips).getByRole("button", { name: "Playlists" }));
  expect(screen.getByText("No playlists in these results.")).toBeInTheDocument();
  expect(search).toHaveBeenCalledTimes(1);
});

test("a playable top result plays on its own", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({ search: () => ({ items: searchResults.items.slice(1), continuation: null }), playQueue });
  renderWithProviders(<Harness initial="test query" />);

  const top = await screen.findByRole("region", { name: "Top result" });
  await userEvent.click(within(top).getByRole("button", { name: "Play" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [
      {
        video_id: "test-vid-11",
        title: "Found song",
        artists: [{ name: "Test artist", browse_id: "UCtest-artist" }],
        thumbnail_url: null,
      },
    ],
    startIndex: 0,
  });
});

test("clicking a song queues that group's songs", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({ search: () => searchResults, playQueue });
  renderWithProviders(<Harness initial="test query" />);

  await userEvent.click(await screen.findByRole("button", { name: "Play Another song" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [
      {
        video_id: "test-vid-11",
        title: "Found song",
        artists: [{ name: "Test artist", browse_id: "UCtest-artist" }],
        thumbnail_url: null,
      },
      { video_id: "test-vid-13", title: "Another song", artists: [], thumbnail_url: null },
    ],
    startIndex: 1,
  });
});

test("suggestions are debounced and selectable with the keyboard", async () => {
  const searchSuggestions = vi.fn(({ input }: { input: string }) => [`${input} live`, `${input} remix`]);
  const search = vi.fn(() => searchResults);
  mockCommands({ searchSuggestions, search });
  renderWithProviders(<Harness />);

  await userEvent.type(field(), "lofi");
  const listbox = await screen.findByRole("listbox", { name: "Suggestions" });
  expect(within(listbox).getAllByRole("option").map((option) => option.textContent)).toEqual([
    "lofi live",
    "lofi remix",
  ]);
  // One request for the settled text, not one per keystroke.
  expect(searchSuggestions).toHaveBeenCalledTimes(1);
  expect(searchSuggestions).toHaveBeenCalledWith({ input: "lofi" });
  expect(field()).toHaveAttribute("aria-expanded", "true");

  await userEvent.keyboard("{ArrowDown}{ArrowDown}");
  const remix = screen.getByRole("option", { name: "lofi remix" });
  expect(remix).toHaveAttribute("aria-selected", "true");
  expect(field()).toHaveAttribute("aria-activedescendant", remix.id);

  await userEvent.keyboard("{Enter}");
  expect(search).toHaveBeenCalledWith({ query: "lofi remix" });
  expect(field()).toHaveValue("lofi remix");
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
});

test("clicking a suggestion searches for it and Escape closes the list", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ searchSuggestions: () => ["first idea", "second idea"], search });
  renderWithProviders(<Harness />);

  await userEvent.type(field(), "idea");
  await screen.findByRole("listbox");
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();

  await userEvent.type(field(), "s");
  await userEvent.click(await screen.findByRole("option", { name: "second idea" }));
  expect(search).toHaveBeenCalledWith({ query: "second idea" });
});

test("no results and errors are explained", async () => {
  let calls = 0;
  mockCommands({
    searchSuggestions: () => [],
    search: () => {
      calls += 1;
      if (calls === 1) throw { kind: "network", message: "network error" };
      return { items: [], continuation: null };
    },
  });
  renderWithProviders(<Harness initial="nothing" />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network error");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("No results for “nothing”.")).toBeInTheDocument();
});

test("an empty query does not search", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ search, searchSuggestions: () => [] });
  renderWithProviders(<Harness />);
  await userEvent.type(field(), "   {Enter}");
  expect(search).not.toHaveBeenCalled();
  expect(screen.getByText(/search for songs/i)).toBeInTheDocument();
});

test.each([{ isComposing: true }, { keyCode: 229 }])(
  "IME commit Enter preserves the suggestion and only the next Enter searches (%j)",
  async (ime) => {
    const search = vi.fn(() => searchResults);
    mockCommands({ searchSuggestions: () => ["한글 추천"], search });
    renderWithProviders(<Harness />);

    await userEvent.type(field(), "한");
    const option = await screen.findByRole("option", { name: "한글 추천" });
    await userEvent.keyboard("{ArrowDown}");
    expect(option).toHaveAttribute("aria-selected", "true");

    expect(fireEvent.keyDown(field(), { key: "Enter", ...ime })).toBe(true);
    expect(option).toHaveAttribute("aria-selected", "true");
    expect(search).not.toHaveBeenCalled();

    // The final input event clears the highlight. Simulate the last character
    // reaching the DOM ahead of React's controlled value on form submission.
    fireEvent.change(field(), { target: { value: "한그" } });
    (field() as HTMLInputElement).value = "한글";
    expect(fireEvent.keyDown(field(), { key: "Enter" })).toBe(true);
    fireEvent.submit(screen.getByRole("search"));
    expect(search).toHaveBeenCalledExactlyOnceWith({ query: "한글" });
  },
);

test("the first ArrowDown after Escape reopens and selects the first suggestion", async () => {
  mockCommands({ searchSuggestions: () => ["first idea", "second idea"] });
  renderWithProviders(<Harness />);
  await userEvent.type(field(), "idea");
  await screen.findByRole("listbox");
  await userEvent.keyboard("{Escape}{ArrowDown}");
  const first = screen.getByRole("option", { name: "first idea" });
  expect(first).toHaveAttribute("aria-selected", "true");
  expect(field()).toHaveAttribute("aria-activedescendant", first.id);
});

test("the search field limits input to 200 characters and announces searching", () => {
  mockCommands({ search: () => new Promise(() => {}) });
  renderWithProviders(<Harness initial="test query" />);
  expect(field()).toHaveAttribute("maxlength", "200");
  expect(screen.getByRole("status")).toHaveTextContent("Searching…");
});
