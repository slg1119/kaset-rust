import type { SearchFilter, SearchItem, SearchResults } from "../bindings";
import { isPlayable, itemKey } from "../music/items";

export type ResultGroup = {
  title: string;
  items: SearchItem[];
};

const GROUPS: { title: string; kinds: SearchItem["kind"][] }[] = [
  { title: "Songs", kinds: ["song"] },
  { title: "Videos", kinds: ["video"] },
  { title: "Albums", kinds: ["album"] },
  { title: "Artists", kinds: ["artist", "profile"] },
  { title: "Playlists", kinds: ["playlist"] },
  { title: "Podcasts", kinds: ["podcast"] },
  { title: "Episodes", kinds: ["episode"] },
];

/** Results grouped by type in a fixed order, keeping server order inside each group. */
export function groupResults(items: SearchItem[]): ResultGroup[] {
  return GROUPS.map((group) => ({
    title: group.title,
    items: items.filter((item) => group.kinds.includes(item.kind)),
  })).filter((group) => group.items.length > 0);
}

/** Server-side filter chips in display order. */
export const FILTERS = [
  { value: "all", label: "All" },
  { value: "songs", label: "Songs" },
  { value: "albums", label: "Albums" },
  { value: "artists", label: "Artists" },
  { value: "playlists", label: "Playlists" },
  { value: "videos", label: "Videos" },
  { value: "podcasts", label: "Podcasts" },
  { value: "episodes", label: "Episodes" },
  { value: "profiles", label: "Profiles" },
] as const satisfies readonly { value: "all" | SearchFilter; label: string }[];

export type Filter = (typeof FILTERS)[number]["value"];

/** Keep the first result for an identity, including overlaps across pages. */
export function mergeSearchPages(pages: SearchResults[]): SearchItem[] {
  const items = new Map<string, SearchItem>();
  for (const page of pages) {
    for (const item of page.items) {
      const key = searchIdentity(item);
      if (!items.has(key)) items.set(key, item);
    }
  }
  return [...items.values()];
}

export function searchIdentity(item: SearchItem): string {
  if (isPlayable(item)) return `video:${item.video_id}`;
  if (item.kind === "profile") return `artist:${item.browse_id}`;
  if (item.kind === "playlist") return `playlist:${item.browse_id.replace(/^VL/, "")}`;
  return itemKey(item);
}

/** The kind label shown on the top-result card. */
export const KIND_LABEL: Record<SearchItem["kind"], string> = {
  song: "Song",
  video: "Video",
  episode: "Episode",
  album: "Album",
  playlist: "Playlist",
  artist: "Artist",
  profile: "Profile",
  podcast: "Podcast",
};
