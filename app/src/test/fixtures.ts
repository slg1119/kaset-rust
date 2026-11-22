import type { Account, FeedPage, SearchResults } from "../bindings";

export const home: FeedPage = {
  chips: [],
  sections: [
    {
      title: "Quick picks",
      items: [
        {
          kind: "song",
          video_id: "test-vid-01",
          title: "First song",
          artists: [{ name: "Test artist", browse_id: null }],
          thumbnail_url: "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
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
    {
      title: "Mixed for you",
      items: [
        {
          kind: "album",
          browse_id: "test-album",
          title: "Test album",
          subtitle: "Album • 2024",
          thumbnail_url: "https://lh3.googleusercontent.com/test-album=w226-h226",
        },
        {
          kind: "song",
          video_id: "test-vid-03",
          title: "Mixed song",
          artists: [],
          thumbnail_url: null,
        },
        {
          kind: "artist",
          browse_id: "UCtest-artist",
          title: "Round artist",
          subtitle: "1.2M subscribers",
          thumbnail_url: "https://yt3.ggpht.com/test-artist=s226",
        },
      ],
    },
    {
      title: "Listen again",
      items: [
        {
          kind: "playlist",
          browse_id: "VLtest-playlist",
          title: "Test playlist",
          subtitle: null,
          thumbnail_url: "http://example.com/not-allowed.jpg",
        },
      ],
    },
  ],
  continuation: null,
};

export const account: Account = {
  name: "Test User",
  handle: "@testuser",
  photo_url: "https://yt3.ggpht.com/test-avatar=s88",
};

export const searchResults: SearchResults = {
  items: [
    {
      kind: "artist",
      browse_id: "UCtest-artist",
      title: "Test artist",
      subtitle: "42K subscribers",
      thumbnail_url: null,
    },
    {
      kind: "song",
      video_id: "test-vid-11",
      title: "Found song",
      artists: [{ name: "Test artist", browse_id: "UCtest-artist" }],
      thumbnail_url: null,
    },
    {
      kind: "video",
      video_id: "test-vid-12",
      title: "Found video",
      artists: [],
      thumbnail_url: null,
    },
    {
      kind: "song",
      video_id: "test-vid-13",
      title: "Another song",
      artists: [],
      thumbnail_url: null,
    },
    {
      kind: "album",
      browse_id: "MPREtest-album",
      title: "Found album",
      subtitle: "Test artist • 2024",
      thumbnail_url: null,
    },
  ],
  continuation: null,
};
