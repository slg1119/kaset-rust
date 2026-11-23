import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "./bindings";
import { idleSnapshot } from "./player/testing";
import { account, home } from "./test/fixtures";
import { renderApp } from "./test/render";

test("signed out shows the sign-in screen", async () => {
  mockCommands({ authStatus: () => "signed_out" });
  renderApp();
  expect(await screen.findByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("signed in shows the shell with the home feed and the account", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => account,
    trackNotifications: () => true,
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
  const nav = screen.getByRole("navigation", { name: "Main" });
  expect(within(nav).getAllByRole("button").map((button) => button.textContent)).toEqual([
    "Search",
    "Home",
    "Explore",
    "Charts",
    "Moods & Genres",
    "New releases",
    "Podcasts",
    "History",
    "Playlists",
    "Liked songs",
    "Albums",
    "Artists",
    "Sign out",
  ]);
  expect(within(nav).getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
  const accountBlock = within(nav).getByRole("region", { name: "Account" });
  expect(await within(accountBlock).findByText("Test User")).toBeInTheDocument();
  expect(within(accountBlock).getByText("@testuser")).toBeInTheDocument();
  expect(await within(accountBlock).findByRole("switch", { name: "Notifications" })).toBeChecked();
});

test("an account failure falls back to a generic label", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => {
      throw { kind: "parse", message: "could not parse" };
    },
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  const accountBlock = await screen.findByRole("region", { name: "Account" });
  expect(within(accountBlock).getByText("Signed in")).toBeInTheDocument();
  await screen.findByRole("heading", { name: "Quick picks" });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("auth://changed switches screens", async () => {
  mockCommands({
    authStatus: () => "signed_out",
    getFeed: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  await screen.findByRole("button", { name: "Sign in with Google" });

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();
});

test("an auth_expired home error routes to sign-in and a new sign-in refetches", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => "signed_in",
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    getFeed: () => {
      calls += 1;
      if (calls === 1) throw { kind: "auth_expired", message: "authentication expired" };
      return home;
    },
  });
  renderApp();
  await vi.waitFor(() => expect(calls).toBe(1));
  // Rust emits auth://changed alongside the auth_expired rejection.
  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
  expect(calls).toBe(2);
});

test("an authStatus failure is shown with a retry", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => {
      calls += 1;
      if (calls === 1) throw { kind: "internal", message: "store unavailable" };
      return "signed_out";
    },
  });
  renderApp();
  expect(await screen.findByRole("alert")).toHaveTextContent("store unavailable");
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(await screen.findByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("sign out calls the command", async () => {
  const signOut = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    signOut,
  });
  renderApp();
  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(signOut).toHaveBeenCalledTimes(1);
});

test("a sign-out failure is a notice and the feed stays", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    signOut: () => {
      throw { kind: "internal", message: "keychain denied" };
    },
  });
  renderApp();
  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not sign out: keychain denied");
  expect(screen.getByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
});

test("the sidebar switches between Home and Search", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  await screen.findByRole("heading", { name: "Quick picks" });
  await userEvent.click(screen.getByRole("button", { name: "Search" }));
  expect(screen.getByRole("heading", { level: 1, name: "Search" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Search" })).toHaveAttribute("aria-current", "page");
  expect(screen.getByRole("combobox", { name: "Search YouTube Music" })).toHaveFocus();
  await userEvent.click(screen.getByRole("button", { name: "Home" }));
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
});

test("keyboard shortcuts control playback and navigation", async () => {
  const togglePlayback = vi.fn(() => null);
  const nextTrack = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getFeed: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    togglePlayback,
    nextTrack,
  });
  renderApp();
  await screen.findByRole("heading", { name: "Quick picks" });

  await userEvent.keyboard(" ");
  expect(togglePlayback).toHaveBeenCalledTimes(1);
  await userEvent.keyboard("{Control>}{ArrowRight}{/Control}");
  expect(nextTrack).toHaveBeenCalledTimes(1);
  await userEvent.keyboard("{Control>}f{/Control}");
  expect(screen.getByRole("combobox", { name: "Search YouTube Music" })).toHaveFocus();
  // Space types in the search field instead of toggling playback.
  await userEvent.keyboard(" ");
  expect(togglePlayback).toHaveBeenCalledTimes(1);
});
