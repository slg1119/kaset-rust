# ADR-0045: UI shell, search, and server-data caching in the Tauri app

## Status

Accepted (2026-11-22, milestone M4 of the Tauri port, ADR-0042).

## Context

M2 and M3 left placeholder React screens: a home list of song buttons, a
fixed-position player bar, and errors handled per component. M4 builds the
shell the MVP screens live in (sidebar, Home, Search, player bar) and adds
search to `ytm-core`. Live search in the developer's region needs a signed-in
cookie file, so the search parser was ported from the Swift parser and its
sanitized fixtures and is verified against live responses with `ytm-explorer`
afterwards.

## Decision

- **Account:** `get_account` reads `account/accounts_list` and returns the
  selected account's name, handle, and photo for the sidebar. No Premium label:
  the Swift app has no subscription signal to port, and none has been verified
  in a live response yet.
- **Search in `ytm-core`:** `parse_search` ports the first-page path of the
  Swift `SearchResponseParser` (server order, first occurrence wins, browse
  semantics before watch fallbacks). `SearchItem` is `song | video | episode`
  (playable `Song`) or `album | playlist | artist | profile | podcast`
  (`BrowseCard`); audiobooks fold into `album`. M5b adds the Swift client's
  server-side filters and parses continuation shelves and action envelopes.
  `parse_search_suggestions` returns text and history suggestions, deduplicated.
  `ytm-explorer` supports `search`, `search-filtered`, `search-continue`, and `suggest`.
- **Shared music feeds** in M5b include Home, Explore, Charts, Moods & Genres,
  New releases, History, and category pages. Mood and genre
  buttons open a feed keyed by browse ID and params. Home queries each topic
  by its own params. Infinite queries append continuation shelves and stop
  when a token is missing or repeats. History is stale immediately so it
  refreshes on entry.
- **Commands:** `search`, `search_continuation`, and `search_suggestions` share `signed_in_request`
  with `get_feed` (launch gate, signed-in check, epoch-guarded expiry). Queries
  are trimmed. Empty search and empty suggestion input return no results
  without a request. Input over 200 characters returns `invalid_argument`.
- **Navigation** in `Shell` uses state-based routes for feeds, Search, categories, and
  detail pages. Category and detail routes store their originating route for Back.
- **Detail pages** for albums and playlists open from Home and Search and
  use the signed-in `get_browse_page` command. The page
  cache is keyed by browse ID and stays fresh for 10 minutes. Back restores
  the originating route and Search retains its query and filter. Play, Shuffle,
  and track rows queue up to 500 songs from the loaded page.
- **Podcasts** reuse the shared feed screen with a dedicated sidebar entry.
  Podcast cards open a show from discovery, Home, and Search. The show cache
  is keyed by browse ID and appends episodes on demand, skipping duplicate
  video IDs and stopping when the continuation token is missing or repeats.
- **Server data:** TanStack Query 5 (`@tanstack/react-query` 5.107.1).
  The app uses one client. Stale times are home 5 min, search 10 min,
  suggestions 60 s, and account 30 min. The client disables refetch on window
  focus and retries `network`/`http` errors once. The cache is cleared whenever
  the auth status leaves `signed_in`.
- **Search pagination** keys results by submitted query and filter. Scrolling
  appends unique results and stops when the continuation is missing or repeats.
  Load more provides a keyboard-accessible trigger and retries preserve loaded
  results. Filtered results retain server order, and videos use the song queue.
- **Errors:** one app-wide notice area for action errors (play, controls,
  shortcuts, sign out, player snapshot); screens keep their own load errors with
  Retry. Auth routing stays backend-driven through `auth://changed`.
- **Auth state:** the UI subscribes to `auth://changed` before reading
  `auth_status`, and shows a failed read with Try again instead of treating it
  as signed out.
- **Artwork:** `img-src` allows `*.ytimg.com`, `*.ggpht.com`, and
  `*.googleusercontent.com`; the UI loads only https URLs on those hosts and
  shows a placeholder otherwise or on load failure.
- **Lyrics panel:** the player opens a 360 px glass panel on the right.
  Opening it loads `get_lyrics` through TanStack Query with a ten-minute
  cache keyed by video ID. The panel shows timed lines, plain lyrics with
  attribution, an unavailable state, or a load error with Retry.
- **Keyboard:** Space, ⌘→, ⌘←, ⌘1, ⌘F, ⌘⇧L (Ctrl off macOS), handled in the WebView;
  never ⌘Q/⌘W/⌘H/⌘M/⌘,. Documented in `docs/keyboard-shortcuts.md`.
- **Layout and styling (approved mockup):** dark only, accent `#d91c48`; a
  240 px sidebar (Search, music feeds, account block), a 56 px sticky header, Home as a
  three-column Quick picks grid plus six-column carousels (round artist cards),
  Search with the field in the header, filter chips (All, Songs, Albums,
  Artists, Playlists, Videos, Podcasts, Episodes, Profiles, in English), and a top-result card; a
  floating player bar inset 24 px whose measured height sets the content's
  bottom padding. Plain CSS, no UI framework.
- **Blur:** CSS `backdrop-filter` (with `-webkit-`) on the sticky header, the
  player bar, the suggestions dropdown, and the queue panel.
- **Native vibrancy:** the main window is transparent and dark. At startup
  `window_effects::apply` uses `window-vibrancy` directly — macOS `Sidebar`
  material, Windows 11 Mica — because it reports failure, unlike Tauri's
  config-level `windowEffects`. The `window_vibrancy` command tells the UI,
  which makes only the sidebar translucent when an effect applied and keeps a
  solid sidebar otherwise (Windows 10). `macOSPrivateApi` is not needed.
- **Icon:** `app/scripts/render-icon.swift` renders the Swift app's Icon
  Composer layer to `app/src-tauri/app-icon.png`; `pnpm tauri icon` produces the
  bundle icons.

## Consequences

- Search shape changes on YouTube's side surface as missing results, not
  errors; the parser logs skipped items at debug level and the explorer is the
  first tool to check.
- Routes for profiles are later work.
- Shortcuts work only while the window has focus; global media keys still go
  through souvlaki (ADR-0044). A native menu bar with accelerators is later
  work.
- The icon lacks the Liquid Glass rendering of the Swift app's `.icon` file.
- A transparent main window means every non-sidebar surface must paint its
  own background; a new full-window screen must use `--bg` (as `.center`
  does) or it shows the material.
- Liquid Glass window effects (available in Tauri 2.12.7 on macOS 26) are not
  used yet.

## M5b artist pages

Artist cards and linked track credits open reusable artist routes from feeds,
search, detail pages, and Now Playing. The shell keeps the source route for
Back. See all uses the shelf’s browse endpoint and caches results by ID and
params. The hero reuses Artwork and the shelves reuse the feed components.

## M5b library and playlist detail

The Library sidebar opens saved playlists, saved albums, followed artists, and
the shared playlist detail for Liked songs (`VLLM`). Collection cards reuse
the feed grid. Detail uses TanStack Query to append tracks as the user scrolls.
Play and Shuffle fetch through the final page or the 500-song queue limit.
Repeated cursors end pagination. Library collections load up to 50 pages and
keep their loaded cards if a later page fails.

Routes covered by a detail stay mounted with their own scroll containers.
The covered screen is inert and hidden from assistive technology. Back
reuses its scroll container and returns focus to the control that opened
the detail.

## M5b track menus and queue editing

Track rows and cards share an ellipsis menu that also opens on right-click.
The queue panel supports jumping to a track, removing and reordering upcoming
tracks, and clearing upcoming tracks. Keyboard reordering keeps focus on the
moved occurrence. The queue button exposes its position as
an accessible description. Surfaces that use backdrop blur have a solid
background fallback.

## M5b account actions

Track ratings and library status share the song-metadata query. Tauri keeps
feedback tokens in an epoch-scoped cache and accepts only a video ID and a
library action from React. Playlist removal uses the server’s occurrence ID
so duplicate videos can be edited individually.

Playlist creation and deletion use native HTML dialogs. The single action
notice follows the open dialog. Rating, library, playlist, and subscription
changes update the displayed state optimistically, restore it on failure,
and invalidate the affected queries on success.

## M5b feed and search pagination

Feed and search stop requesting more pages when the latest page has no new
item identities, even if the server supplies a new continuation token.
