import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands, type FeedPage } from "../bindings";
import { idleSnapshot } from "../player/testing";
import { home } from "../test/fixtures";
import { renderApp, renderWithProviders } from "../test/render";
import { FeedScreen } from "./FeedScreen";

const firstPage: FeedPage = {
  ...home,
  chips: [{ title: "Focus", params: "test-chip%3D" }],
  continuation: "test-continuation",
};

const nextPage: FeedPage = {
  sections: [{ title: "More picks", items: home.sections[0].items }],
  chips: [],
  continuation: null,
};

test("Home loads a selected chip and All restores the original feed", async () => {
  const getFeedChip = vi.fn(() => nextPage);
  mockCommands({ getFeed: () => firstPage, getFeedChip });
  renderWithProviders(<FeedScreen source={{ kind: "home" }} title="Home" onOpenPage={() => {}} onOpenCategory={() => {}} />);

  await userEvent.click(await screen.findByRole("button", { name: "Focus" }));
  expect(getFeedChip).toHaveBeenCalledWith({ params: "test-chip%3D" });
  expect(await screen.findByRole("region", { name: "More picks" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Focus" })).toHaveAttribute("aria-pressed", "true");

  await userEvent.click(screen.getByRole("button", { name: "All" }));
  expect(await screen.findByRole("region", { name: "Quick picks" })).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "More picks" })).not.toBeInTheDocument();
});

test("the sentinel appends the next page and a repeated token stops pagination", async () => {
  const getFeedContinuation = vi.fn(() => ({ ...nextPage, continuation: "test-continuation" }));
  const observe = vi.fn<(element: Element) => void>();
  let notify: IntersectionObserverCallback | undefined;
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: IntersectionObserverCallback) {
      notify = callback;
    }
    observe = observe;
    disconnect() {}
  });
  try {
    mockCommands({ getFeed: () => firstPage, getFeedContinuation });
    renderWithProviders(<FeedScreen source={{ kind: "home" }} title="Home" onOpenPage={() => {}} onOpenCategory={() => {}} />);
    await screen.findByRole("button", { name: "Load more" });
    await vi.waitFor(() => expect(observe).toHaveBeenCalledTimes(1));
    act(() => notify?.([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver));
    expect(await screen.findByRole("region", { name: "More picks" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Quick picks" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
    expect(getFeedContinuation.mock.calls).toEqual([[{ token: "test-continuation" }]]);
  } finally {
    vi.unstubAllGlobals();
  }
});

test("a category tile opens its feed and Back returns to Moods & Genres", async () => {
  const getFeedCategory = vi.fn(() => nextPage);
  mockCommands({
    authStatus: () => "signed_in",
    windowVibrancy: () => false,
    getAccount: () => null,
    playerSnapshot: () => idleSnapshot,
    getFeed: ({ kind }) => kind === "moods_and_genres" ? {
      chips: [],
      continuation: null,
      sections: [{ title: "Genres", items: [{
        kind: "category", title: "Jazz", browse_id: "test-category", params: "test-jazz", color: "#112233",
      }] }],
    } : firstPage,
    getFeedCategory,
  });
  renderApp();

  await userEvent.click(await screen.findByRole("button", { name: "Moods & Genres" }));
  const category = await screen.findByRole("button", { name: "Jazz" });
  expect(category.style.getPropertyValue("--tile-color")).toBe("#112233");
  await userEvent.click(category);
  expect(await screen.findByRole("heading", { name: "Jazz" })).toBeInTheDocument();
  expect(getFeedCategory).toHaveBeenCalledWith({ browseId: "test-category", params: "test-jazz" });

  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByRole("heading", { name: "Moods & Genres" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Moods & Genres" })).toHaveAttribute("aria-current", "page");
});

test.each([false, true])("stops feed paging when a fresh token adds nothing (empty: %s)", async (empty) => {
  const getFeedContinuation = vi.fn(() => ({
    ...nextPage,
    sections: empty ? [] : nextPage.sections,
    continuation: "test-fresh-token",
  }));
  mockCommands({ getFeed: () => firstPage, getFeedContinuation });
  renderWithProviders(<FeedScreen source={{ kind: "home" }} title="Home" onOpenPage={() => {}} onOpenCategory={() => {}} />);
  await userEvent.click(await screen.findByRole("button", { name: "Load more" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument());
  expect(getFeedContinuation).toHaveBeenCalledExactlyOnceWith({ token: "test-continuation" });
});
