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
  (`BrowseCard`); audiobooks fold into `album`. One unfiltered request; filtered
  searches and continuation pages are later work. `parse_search_suggestions`
  returns text and history suggestions, deduplicated. `ytm-explorer` gains
  `search` and `suggest`.
- **Home shelves:** the home parser reads carousel, music shelf, card shelf,
  immersive carousel, grid, and item-section shelves, like the Swift parser;
  mood/genre buttons are skipped.
- **Commands:** `search` and `search_suggestions` share `signed_in_request`
  with `get_home` (launch gate, signed-in check, epoch-guarded expiry). Queries
  are trimmed; empty search and over-200-character input are
  `invalid_argument`; empty suggestion input returns nothing without a request.
- **Navigation:** state-based routes in `Shell` (`home`, `search`), no router
  library: a desktop WebView has no URLs to share, and two screens do not need
  one. M5 extends the route type with a stack for detail pages and Back.
- **Server data:** TanStack Query 5 (`@tanstack/react-query` 5.107.1). One
  client per app; `staleTime` home 5 min, search 10 min, suggestions 60 s; no
  refetch on window focus; one retry for `network`/`http` errors only; the cache
  is cleared whenever the auth status leaves `signed_in`.
- **Errors:** one app-wide notice area for action errors (play, controls,
  shortcuts, sign out, player snapshot); screens keep their own load errors with
  Retry. Auth routing stays backend-driven through `auth://changed`.
- **Auth state:** the UI subscribes to `auth://changed` before reading
  `auth_status`, and shows a failed read with Try again instead of treating it
  as signed out.
- **Artwork:** `img-src` allows `*.ytimg.com`, `*.ggpht.com`, and
  `*.googleusercontent.com`; the UI loads only https URLs on those hosts and
  shows a placeholder otherwise or on load failure.
- **Keyboard:** Space, ⌘→, ⌘←, ⌘1, ⌘F (Ctrl off macOS), handled in the WebView;
  never ⌘Q/⌘W/⌘H/⌘M/⌘,. Documented in `docs/keyboard-shortcuts.md`.
- **Layout and styling (approved mockup):** dark only, accent `#d91c48`; a
  240 px sidebar (Search, Home, account block), a 56 px sticky header, Home as a
  three-column Quick picks grid plus six-column carousels (round artist cards),
  Search with the field in the header, filter chips (All, Songs, Albums,
  Artists, Playlists; English until localization), and a top-result card; a
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
- Browse results (albums, artists, playlists) are visible but not interactive
  until M5 adds detail pages.
- Shortcuts work only while the window has focus; global media keys still go
  through souvlaki (ADR-0044). A native menu bar with accelerators is later
  work.
- The icon lacks the Liquid Glass rendering of the Swift app's `.icon` file.
- A transparent main window means every non-sidebar surface must paint its
  own background; a new full-window screen must use `--bg` (as `.center`
  does) or it shows the material.
- Liquid Glass window effects (available in Tauri 2.12.7 on macOS 26) are not
  used yet.
