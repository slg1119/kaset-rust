import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { testSong } from "../player/testing";
import { renderWithProviders } from "../test/render";
import { SongMenu } from "./SongMenu";

test("opens from the trigger and the track, with keyboard navigation and focus restoration", async () => {
  const enqueue = vi.fn(() => "Added" as const);
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

test.each(["Play next", "Add to queue"])("reports a queue no-op after %s", async (action) => {
  mockCommands({ insertNext: () => "AlreadyQueued", enqueue: () => "AlreadyQueued" });
  renderWithProviders(<SongMenu song={testSong}>Track</SongMenu>);
  await userEvent.click(screen.getByRole("button", { name: "More actions for Test song" }));
  await userEvent.click(screen.getByRole("menuitem", { name: action }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Already in the queue");
});

test.each(["Play next", "Add to queue"])("reports the cap after %s", async (action) => {
  mockCommands({ insertNext: () => "Full", enqueue: () => "Full" });
  renderWithProviders(<SongMenu song={testSong}>Track</SongMenu>);
  await userEvent.click(screen.getByRole("button", { name: "More actions for Test song" }));
  await userEvent.click(screen.getByRole("menuitem", { name: action }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The queue is full");
});

test("uses only the song and action for library feedback, then rolls back a rejection", async () => {
  let reject!: (error: Error) => void;
  const editLibrary = vi.fn(() => new Promise<null>((_, fail) => { reject = fail; }));
  mockCommands({
    songMetadata: () => ({ like_status: null, in_library: false, can_add: true, can_remove: true }),
    editLibrary,
  });
  renderWithProviders(<SongMenu song={testSong}>Track</SongMenu>);
  const trigger = screen.getByRole("button", { name: "More actions for Test song" });
  await userEvent.click(trigger);
  const add = screen.getByRole("menuitem", { name: "Add to library" });
  await waitFor(() => expect(add).toBeEnabled());
  await userEvent.click(add);
  expect(editLibrary).toHaveBeenCalledWith({ edit: { video_id: testSong.video_id, action: "add" } });
  await userEvent.click(trigger);
  expect(screen.getByRole("menuitem", { name: "Remove from library" })).toBeDisabled();
  await act(() => reject(new Error("Library edit failed")));
  expect(await screen.findByRole("alert")).toHaveTextContent("Library edit failed");
  await waitFor(() => expect(screen.getByRole("menuitem", { name: "Add to library" })).toBeEnabled());
});

test("lists playlist targets, rolls back an unsuccessful add, and seeds a new playlist", async () => {
  let reject!: (error: Error) => void;
  const addToPlaylist = vi.fn(() => new Promise<null>((_, fail) => { reject = fail; }));
  const createPlaylist = vi.fn(() => "PLcreated");
  mockCommands({
    songMetadata: () => ({ like_status: null, in_library: false, can_add: true, can_remove: false }),
    addToPlaylistOptions: () => ({
      title: "Add to playlist", can_create_playlist: true,
      options: [{ playlist_id: "PLmix", title: "Mix", subtitle: null, thumbnail_url: null, is_selected: false, privacy_status: null }],
    }),
    addToPlaylist,
    createPlaylist,
  });
  renderWithProviders(<SongMenu song={testSong}>Track</SongMenu>);
  const trigger = screen.getByRole("button", { name: "More actions for Test song" });
  await userEvent.click(trigger);
  await userEvent.click(screen.getByRole("menuitem", { name: "Add to playlist…" }));
  const dialog = within(screen.getByRole("dialog", { name: "Add to playlist" }));
  await userEvent.click(await dialog.findByRole("button", { name: "Mix" }));
  expect(addToPlaylist).toHaveBeenCalledWith({ videoId: testSong.video_id, playlistId: "PLmix" });
  expect(await dialog.findByRole("button", { name: "Mix, added" })).toBeDisabled();
  await act(() => reject(new Error("Playlist edit failed")));
  expect(await dialog.findByRole("alert")).toHaveTextContent("Playlist edit failed");
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(await dialog.findByRole("button", { name: "Mix" })).toBeEnabled();

  await userEvent.click(dialog.getByRole("button", { name: "New playlist" }));
  expect(screen.getByRole("dialog", { name: "New playlist" })).toBeInTheDocument();
  await userEvent.type(dialog.getByRole("textbox", { name: "Title" }), "Test playlist");
  await userEvent.type(dialog.getByRole("textbox", { name: "Description" }), "Test description");
  await userEvent.selectOptions(dialog.getByRole("combobox", { name: "Privacy" }), "UNLISTED");
  await userEvent.click(dialog.getByRole("button", { name: "Create playlist" }));
  expect(createPlaylist).toHaveBeenCalledWith({
    title: "Test playlist", description: "Test description", privacy: "UNLISTED", videoIds: [testSong.video_id],
  });
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});

test("validates the trimmed playlist title at the 150-character limit", async () => {
  const createPlaylist = vi.fn(() => "PLcreated");
  mockCommands({
    addToPlaylistOptions: () => ({ title: "Add to playlist", options: [], can_create_playlist: true }),
    createPlaylist,
  });
  renderWithProviders(<SongMenu song={testSong}>Track</SongMenu>);
  await userEvent.click(screen.getByRole("button", { name: "More actions for Test song" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "Add to playlist…" }));
  await userEvent.click(await screen.findByRole("button", { name: "New playlist" }));
  const title = "가".repeat(150);
  fireEvent.change(screen.getByRole("textbox", { name: "Title" }), { target: { value: `  ${title}  ` } });
  const submit = screen.getByRole("button", { name: "Create playlist" });
  expect(submit).toBeEnabled();
  await userEvent.click(submit);
  expect(createPlaylist).toHaveBeenCalledWith({
    title, description: "", privacy: "PRIVATE", videoIds: [testSong.video_id],
  });
});
