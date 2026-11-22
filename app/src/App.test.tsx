import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { App } from "./App";
import { events, mockCommands, type HomePage } from "./bindings";

const home: HomePage = {
  sections: [
    { title: "Listen again", items: [] },
    {
      title: "Quick picks",
      items: [
        {
          kind: "song",
          video_id: "test-video",
          title: "Test song",
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
  mockCommands({ authStatus: () => "signed_in", getHome: () => home });
  render(<App />);
  expect(await screen.findByText("Listen again (0)")).toBeInTheDocument();
  expect(screen.getByText("Quick picks (1)")).toBeInTheDocument();
});

test("auth://changed switches screens", async () => {
  mockCommands({ authStatus: () => "signed_out", getHome: () => home });
  render(<App />);
  await screen.findByRole("button", { name: "Sign in with Google" });

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByText("Quick picks (1)")).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();
});

test("home errors offer a retry", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => "signed_in",
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
  mockCommands({ authStatus: () => "signed_in", getHome: () => home, signOut });
  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(signOut).toHaveBeenCalledTimes(1);
});
