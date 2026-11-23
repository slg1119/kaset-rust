import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { testSong } from "../player/testing";
import { renderWithProviders } from "../test/render";
import { SongMenu } from "./SongMenu";

test("opens from the trigger and the track, with keyboard navigation and focus restoration", async () => {
  const enqueue = vi.fn(() => null);
  mockCommands({ enqueue });
  renderWithProviders(<SongMenu song={testSong}><span>Test track row</span></SongMenu>);
  const trigger = screen.getByRole("button", { name: "More actions for Test song" });
  await userEvent.click(trigger);
  expect(screen.getByRole("menuitem", { name: "Play next" })).toHaveFocus();
  expect(screen.queryByRole("menuitem", { name: "Go to artist" })).not.toBeInTheDocument();
  expect(screen.queryByRole("menuitem", { name: "Go to album" })).not.toBeInTheDocument();
  await userEvent.keyboard("{ArrowDown}{Enter}");
  expect(enqueue).toHaveBeenCalledWith({ songs: [testSong] });
  expect(trigger).toHaveFocus();
  fireEvent.contextMenu(screen.getByText("Test track row"), { clientX: 40, clientY: 40 });
  expect(screen.getByRole("menu")).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

test("uses the notice area for a queue action failure and sends the song to radio", async () => {
  const insertNext = vi.fn().mockRejectedValue(new Error("Queue edit failed"));
  const startRadio = vi.fn(() => null);
  mockCommands({ insertNext, startRadio });
  renderWithProviders(<SongMenu song={testSong}>Track</SongMenu>);
  const trigger = screen.getByRole("button", { name: "More actions for Test song" });
  await userEvent.click(trigger);
  await userEvent.click(screen.getByRole("menuitem", { name: "Play next" }));
  expect(insertNext).toHaveBeenCalledWith({ songs: [testSong] });
  expect(await screen.findByRole("alert")).toHaveTextContent("Queue edit failed");

  await userEvent.click(trigger);
  await userEvent.click(screen.getByRole("menuitem", { name: "Start radio" }));
  expect(startRadio).toHaveBeenCalledWith({ song: testSong });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("routes through known artist and album destinations", async () => {
  const openArtist = vi.fn();
  const openAlbum = vi.fn();
  const song = { ...testSong, artists: [{ name: "Test artist", browse_id: "test-artist" }] };
  renderWithProviders(<SongMenu song={song} onOpenArtist={openArtist} onOpenAlbum={openAlbum}>Track</SongMenu>);
  const trigger = screen.getByRole("button", { name: "More actions for Test song" });
  await userEvent.click(trigger);
  await userEvent.click(screen.getByRole("menuitem", { name: "Go to artist" }));
  expect(openArtist).toHaveBeenCalledWith("test-artist", "Test artist");
  await userEvent.click(trigger);
  await userEvent.click(screen.getByRole("menuitem", { name: "Go to album" }));
  expect(openAlbum).toHaveBeenCalledTimes(1);
});
