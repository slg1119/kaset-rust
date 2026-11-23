import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { renderWithProviders } from "../test/render";
import { Sidebar } from "./Sidebar";

test("the account switch reads and saves the track notification preference", async () => {
  let enabled = false;
  const setTrackNotifications = vi.fn((value: { enabled: boolean }) => {
    enabled = value.enabled;
    return null;
  });
  mockCommands({
    getAccount: () => null,
    trackNotifications: () => enabled,
    setTrackNotifications,
  });
  renderWithProviders(<Sidebar route={{ kind: "home" }} onNavigate={vi.fn()} />);

  const toggle = await screen.findByRole("switch", { name: "Notifications" });
  expect(toggle).not.toBeChecked();
  await userEvent.click(toggle);
  await waitFor(() => expect(toggle).toBeChecked());
  expect(setTrackNotifications).toHaveBeenCalledWith({ enabled: true });
});
