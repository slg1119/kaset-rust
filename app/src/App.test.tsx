import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { App } from "./App";
import { events, mockCommands, type HomePage } from "./bindings";
import { idleSnapshot, playingState } from "./player/testing";

const home: HomePage = {
  sections: [
    { title: "Listen again", items: [] },
    {
      title: "Quick picks",
      items: [
        {
          kind: "song",
          video_id: "test-vid-01",
          title: "First song",
          artists: [{ name: "Test artist", browse_id: null }],
          thumbnail_url: null,
        },
        {
          kind: "album",
          browse_id: "test-album",
          title: "Test album",
          subtitle: null,
          thumbnail_url: null,
        },
        {
          kind: "song",
          video_id: "test-vid-02",
          title: "Second song",
          artists: [],
          thumbnail_url: null,
        },
      ],
    },
  ],
  continuation: null,
};

test("signed out shows the sign-in screen", async () => {
  mockCommands({ authStatus: () => "signed_out" });
  render(<App />);
  expect(await screen.findByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("signed in lists home section titles", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
  });
  render(<App />);
  expect(await screen.findByText("Listen again (0)")).toBeInTheDocument();
  expect(screen.getByText("Quick picks (3)")).toBeInTheDocument();
});

test("auth://changed switches screens", async () => {
  mockCommands({
    authStatus: () => "signed_out",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
  });
  render(<App />);
  await screen.findByRole("button", { name: "Sign in with Google" });

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByText("Quick picks (3)")).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();
});

test("home errors offer a retry", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => "signed_in",
    playerSnapshot: () => idleSnapshot,
    getHome: () => {
      calls += 1;
      if (calls === 1) throw { kind: "network", message: "network error" };
      return home;
    },
  });
  render(<App />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network error");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("Listen again (0)")).toBeInTheDocument();
});

test("sign out calls the command", async () => {
  const signOut = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
    signOut,
  });
  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(signOut).toHaveBeenCalledTimes(1);
});

test("clicking a song plays its shelf's songs from that song", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
    playQueue,
  });
  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: "Second song" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [
      {
        video_id: "test-vid-01",
        title: "First song",
        artists: [{ name: "Test artist", browse_id: null }],
        thumbnail_url: null,
      },
      { video_id: "test-vid-02", title: "Second song", artists: [], thumbnail_url: null },
    ],
    startIndex: 1,
  });
  expect(screen.getByRole("button", { name: "First song · Test artist" })).toBeInTheDocument();
});

test("the player bar appears when playback starts", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
  });
  render(<App />);
  await screen.findByText("Listen again (0)");
  expect(screen.queryByRole("contentinfo", { name: "Player" })).not.toBeInTheDocument();

  await act(() => events.playerState.emit({ state: playingState() }));
  expect(await screen.findByRole("contentinfo", { name: "Player" })).toHaveTextContent("Test song");
});

test("a failed play shows the error", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
    playQueue: () => {
      throw { kind: "not_signed_in", message: "Sign in to continue." };
    },
  });
  render(<App />);
  await userEvent.click(await screen.findByRole("button", { name: "Second song" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Sign in to continue.");
});
