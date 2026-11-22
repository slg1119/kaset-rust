import type { SearchItem } from "../bindings";

export type ResultGroup = {
  title: string;
  items: SearchItem[];
  /** Songs, videos, and episodes play as a queue from the clicked item. */
  playable: boolean;
};

const GROUPS: { title: string; kinds: SearchItem["kind"][]; playable: boolean }[] = [
  { title: "Songs", kinds: ["song"], playable: true },
  { title: "Videos", kinds: ["video"], playable: true },
  { title: "Albums", kinds: ["album"], playable: false },
  { title: "Artists", kinds: ["artist", "profile"], playable: false },
  { title: "Playlists", kinds: ["playlist"], playable: false },
  { title: "Podcasts", kinds: ["podcast"], playable: false },
  { title: "Episodes", kinds: ["episode"], playable: true },
];

/** Results grouped by type in a fixed order, keeping server order inside each group. */
export function groupResults(items: SearchItem[]): ResultGroup[] {
  return GROUPS.map((group) => ({
    title: group.title,
    playable: group.playable,
    items: items.filter((item) => group.kinds.includes(item.kind)),
  })).filter((group) => group.items.length > 0);
}

export type Filter = "all" | "songs" | "albums" | "artists" | "playlists";

/** The filter chips, in display order. They filter the loaded results; no new request. */
export const FILTERS: { value: Filter; label: string; group?: string }[] = [
  { value: "all", label: "All" },
  { value: "songs", label: "Songs", group: "Songs" },
  { value: "albums", label: "Albums", group: "Albums" },
  { value: "artists", label: "Artists", group: "Artists" },
  { value: "playlists", label: "Playlists", group: "Playlists" },
];

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
