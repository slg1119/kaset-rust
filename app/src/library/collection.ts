import { commands, type LibraryKind } from "../bindings";

export async function fetchLibraryCollection(kind: Exclude<LibraryKind, "songs">) {
  const library = await commands.getLibrary(kind);
  if (library.kind !== "collection") throw new Error("Could not read this library collection.");
  return library.data.flatMap((section) => section.items);
}
