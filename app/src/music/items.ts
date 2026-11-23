import type { ArtistRef, Episode, SearchItem, ShelfItem, Song } from "../bindings";

/** A home or search item: a song-like item or a browse card, tagged by `kind`. */
export type MusicItem = ShelfItem | SearchItem;

/** Items that carry a video id and can be queued. */
export function isPlayable<T extends MusicItem>(item: T): item is Extract<T, { video_id: string }> {
  return "video_id" in item;
}

/** Browse cards with detail screens. */
export type PageCard = Extract<MusicItem, { kind: "album" | "playlist" | "artist" | "podcast" }>;

export function hasBrowsePage(item: MusicItem): item is PageCard {
  return item.kind === "album" || item.kind === "playlist" || item.kind === "artist" || item.kind === "podcast";
}

/** The `Song` that `playQueue` takes, without the `kind` tag. */
export function toSong(item: Song | Episode): Song {
  if (!("artists" in item)) return episodeToSong(item);
  return {
    video_id: item.video_id,
    title: item.title,
    artists: item.artists,
    thumbnail_url: item.thumbnail_url,
    ...((item.is_podcast_episode || ("kind" in item && item.kind === "episode")) && { is_podcast_episode: true }),
  };
}

export function episodeToSong(episode: Episode, showTitle = episode.show_title, artwork: string | null = null): Song {
  return {
    is_podcast_episode: true,
    video_id: episode.video_id,
    title: episode.title,
    artists: showTitle ? [{ name: showTitle, browse_id: null }] : [],
    thumbnail_url: episode.thumbnail_url ?? artwork,
  };
}

export function artistNames(artists: ArtistRef[]): string {
  return artists.map((artist) => artist.name).join(", ");
}

/** The second line under an item's title. */
export function subtitleOf(item: MusicItem): string | null {
  if (isPlayable(item)) return "artists" in item ? artistNames(item.artists) || null : item.show_title;
  return item.kind === "category" ? null : item.subtitle;
}

/** A stable React key for an item within its list. */
export function itemKey(item: MusicItem): string {
  return isPlayable(item) ? `${item.kind}:${item.video_id}` : `${item.kind}:${item.browse_id}`;
}

/** Artists and profiles use round artwork. */
export function shapeOf(item: MusicItem): "square" | "round" {
  return item.kind === "artist" || item.kind === "profile" ? "round" : "square";
}
