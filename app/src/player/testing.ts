import type { PlayerSnapshot, PlayerState, Song } from "../bindings";

export const testSong: Song = {
  video_id: "test-vid-0A",
  title: "Test song",
  artists: [{ name: "Test artist", browse_id: null }],
  thumbnail_url: null,
};

export const idleSnapshot: PlayerSnapshot = {
  state: {
    status: "idle",
    track: null,
    position: 0,
    duration: 0,
    is_ad: false,
    repeat: "off",
    shuffle: false,
    has_previous: false,
    has_next: false,
  },
  queue: { songs: [], current: null },
};

export function playingState(overrides: Partial<PlayerState> = {}): PlayerState {
  return {
    status: "playing",
    track: testSong,
    position: 65,
    duration: 200,
    is_ad: false,
    repeat: "off",
    shuffle: false,
    has_previous: false,
    has_next: true,
    ...overrides,
  };
}
