# Kaset Tauri Port — M4 (UI Shell: Sidebar, Home, Search, Player Bar) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the M2/M3 placeholder screens with the app's UI shell from the approved mockup: a dark theme; a 240 px sidebar (Search and Home with a pink active pill, an account block with name and sign-out at the bottom) over native window vibrancy; a 56 px blurred sticky header; Home with a three-column Quick picks song grid and six-column card carousels; Search with the field in the header, a blurred suggestions dropdown, type filter chips, a top-result card, and grouped results (songs, videos, and episodes playable through `play_queue`); a floating, blurred player bar inset 24 px from the content edges; and a sign-in screen with an expired-session alert — with TanStack Query caching server data, macOS-convention keyboard shortcuts, the real Kaset app icon, and the UI items deferred from the M2/M3 reviews.

**Architecture:** Tasks 1–4 extend `ytm-core` (no Tauri): the home parser learns the shelf types the Swift `HomeResponseParser` handles; a search parser ports the Swift `SearchResponseParser` (first page, server order, first occurrence wins, browse semantics before watch fallbacks); a suggestions parser ports `SearchSuggestionsParser`; an account parser reduces the Swift `AccountsListParser` to name, handle, and photo. The client and `ytm-explorer` gain `search`, `suggest`, and `account`. Task 5 adds the `search`, `search_suggestions`, `get_account`, and `window_vibrancy` commands on a shared `signed_in_request` helper, makes the main window transparent and dark, applies macOS sidebar vibrancy or Windows 11 Mica at startup through `window-vibrancy` (already in the lockfile via Tauri), regenerates the bindings, and widens the image CSP. Tasks 6–8 rebuild the React app: shared building blocks (query client, single notice area, artwork, item cards), the Search screen, then the shell that wires auth, navigation, Home, the floating player bar, and shortcuts. Task 9 replaces the Tauri placeholder icons. Task 10 records the decisions (ADR-0045) and updates AGENTS.md.

**Tech Stack (versions verified 2026-11-22 against the npm registry, the crate sources in `~/.cargo/registry`, the lockfile, and a scratch build):**
- Rust (existing, unchanged): tauri **2.12.7**, tauri-utils 2.10.4, tauri-specta **=2.0.0-rc.29**, specta **=2.0.0-rc.28**, specta-typescript **=0.0.15**, reqwest 0.12, regex 1, insta 1, wiremock 0.6, clap 4.
- Rust (new direct dependency, already in `Cargo.lock` as Tauri's own dependency): **window-vibrancy 0.8.1** (`apply_vibrancy` on macOS, `apply_mica` on Windows; both return `Result`, so the app knows whether the effect applied). No new crate is downloaded.
- Frontend (existing): react **19.3.1**, @tauri-apps/api ~2.12.5, vitest 5.1.1, @testing-library/react 16.3.3, @testing-library/user-event 14.6.13, eslint-plugin-react-hooks 7.1.1, typescript ~5.9.3, vite 8.3.8.
- Frontend (new, the only new npm dependency): **@tanstack/react-query 5.107.1**, pinned exactly (published 2026-11-06; peer `react: ^18 || ^19`, `@types/react: ^18 || ^19`; depends only on `@tanstack/query-core` 5.107.1). 5.108.x exists but is younger than two weeks, so it is not used.
- No router library and no UI framework (Decisions 2 and 9).

**Spec:** `docs/superpowers/specs/2026-11-22-tauri-port-design.md` (milestone 4: "UI shell: sidebar, home, search, player bar"; `app/src` owns "Screens, routing, server-data caching with TanStack Query"). Read with it: ADR-0042, ADR-0043, ADR-0044, `docs/superpowers/spikes/2026-11-22-tauri-playback.md`, and the M3 plan `docs/superpowers/plans/2026-11-22-tauri-port-m3.md` for the conventions of the existing code. The human's approved mockup requirements are restated in the Goal and in Decisions 9–12; there is no mockup file in the repository.

## Global Constraints

- No real credentials anywhere — code, comments, logs, fixtures, docs, output. Placeholders only (`test-sapisid`, `test-cookie`, `test-vid-01`, `Test User`, `test@example.com`). The fixtures copied in Task 2 are already sanitized in the Swift test suite; copy them unchanged. The account name is shown in the UI only; never log it.
- **API Discovery rule (AGENTS.md):** the search, suggestion, and account parsers are ported from the Swift parsers and sanitized fixtures because live requests in the human's region need a signed-in cookie file. Task 4 Step 8 is a required human verification against real responses with `ytm-explorer`; record the result before M5 builds on search.
- `crates/ytm-core` must not depend on Tauri. Parsers skip malformed items with a `tracing::debug!` instead of failing the response.
- Tauri **2.x only**. Commands reach the frontend only through the generated `app/src/bindings.ts`; no hand-written `invoke("…")`. Regenerate with `cargo test -p kaset bindings_are_current` and commit the file in the same task. New commands also go in `build.rs` and `capabilities/default.json`; commit the generated `permissions/autogenerated/*.toml`.
- Every API command goes through `signed_in_request` (launch gate, signed-in check, epoch-guarded `on_error`). HTTP 401/403 → `AuthExpired` → Rust emits `auth://changed`; the UI never routes on its own guess.
- Window effects are applied only on the main thread (Tauri `setup`). Do not set `windowEffects` in `tauri.conf.json` (Tauri cannot report whether it applied) and do not enable `macOSPrivateApi` (a no-op since Tauri 2.12.1; transparency works without it).
- Frontend: plain CSS in `app/src/styles.css`; no UI framework, no router, no new npm dependency beyond @tanstack/react-query 5.107.1. Every `backdrop-filter` also sets `-webkit-backdrop-filter` (WKWebView). Tests use Vitest + Testing Library and `mockCommands` from the bindings; every file must pass `pnpm lint` (eslint-plugin-react-hooks 7 recommended rules, including no synchronous `setState` in effects).
- Keyboard shortcuts follow macOS and Apple Music conventions and never take `⌘Q`, `⌘W`, `⌘H`, `⌘M`, or `⌘,` (Ctrl on Windows). Document them in `docs/keyboard-shortcuts.md` (AGENTS.md rule).
- Codex workers do not launch the GUI app and do not send live requests that need a Google account; runtime checks are in the human checklist at the end. Live requests are read-only.
- Do not modify or delete Swift sources (`Sources/`, `Tests/`, `Package.swift`) or `spikes/`. Copy fixtures from `Tests/KasetTests/Fixtures/`; never move them. Do not run Swift UI tests.
- Rust edition 2024. For touched crates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` must pass. For `app/`: `pnpm lint`, `pnpm typecheck`, `pnpm test` must pass.
- Commit directly on `main` and `git push origin main` at the end of every task (human directive: direct commits, no PRs). Stage only the files the task lists.

## Decisions made in this plan

Recorded in ADR-0045 by Task 10.

1. **Search scope:** one unfiltered `search` request (the mixed first page). Results are `SearchItem` = `song | video | episode` (a `Song`, playable) or `album | playlist | artist | profile | podcast` (a `BrowseCard`). Audiobooks fold into `album`; podcast shows into `podcast`. Filtered searches (Swift `SearchFilterParams`) and continuation pages are not requested; `SearchResults.continuation` is kept for later. Detail pages for browse items come in M5, so those results render without an action.
2. **Navigation:** state-based (`Route = "home" | "search"` in `Shell`), no router. A desktop WebView has no address bar or shareable URLs, the shell has two screens, and M5's detail pages can extend `Route` with a stack for Back. A router (react-router, TanStack Router) would add a dependency and URL handling the app does not need yet; revisit in M5 if the stack grows complex.
3. **TanStack Query:** one `QueryClient` per app (`createQueryClient`), created in `main.tsx` and passed to `App`. `staleTime`: home 5 min, search 10 min, suggestions 60 s, account 30 min; default `gcTime` (5 min). `refetchOnWindowFocus: false` (a desktop window gains focus constantly; Home has a Refresh button). Retry once only for `network` and `http` errors; never for `auth_expired`, `not_signed_in`, `invalid_argument`, `parse`, `rate_limited`, or `internal`. The cache is cleared whenever the auth status leaves `signed_in`, so feeds and the account never cross sessions.
4. **Notices:** one app-wide notice area (`NoticeProvider` + `NoticeBanner`, `role="alert"`, floating below the header) for action errors — play, player controls, shortcuts, sign out, player snapshot failure. A new action dismisses the previous notice. Screens keep their own load-error states with Retry. This fixes the duplicate player-bar/home alerts and keeps a sign-out error from replacing the feed.
5. **Auth hook:** `useAuthStatus` subscribes to `auth://changed` *before* calling `authStatus` (fixes the listen/reply gap), unsubscribes on unmount even if the subscription resolves later, and exposes `error` + `retry` instead of silently treating a failed read as signed out.
6. **Search input:** an ARIA combobox (`role="combobox"`, `aria-autocomplete="list"`, `aria-activedescendant`) in the content header with a listbox popup. Suggestions are requested 250 ms after the last keystroke; ArrowUp/ArrowDown move, Return searches the highlighted suggestion or the typed text, Escape closes, a click searches the suggestion. Suggestion failures are silent (Return still works). Queries are trimmed; empty input never reaches Rust; more than 200 characters is `invalid_argument`.
7. **Artwork and CSP:** `img-src` allows `https://*.ytimg.com` (replacing `https://i.ytimg.com`), `https://*.ggpht.com`, and `https://*.googleusercontent.com` — the hosts in the Swift fixtures and real responses (`i.ytimg.com`, `yt3.googleusercontent.com`, `lh3.googleusercontent.com`, `yt3.ggpht.com`). The frontend's `safeImageUrl` accepts only https URLs on those host families (upgrading protocol-relative URLs) and shows a placeholder otherwise or when loading fails, so the CSP never blocks a request we make. Images use `loading="lazy"` and `referrerPolicy="no-referrer"`. Window transparency and vibrancy need no CSP change.
8. **Keyboard shortcuts:** Space play/pause (not when focus is on a control or text field, not on key repeat), ⌘→ next, ⌘← previous (not in text fields), ⌘1 Home, ⌘F Search (focuses the header search field). ⌘F is the standard macOS "Find/Search" shortcut and Apple Music uses it for Search; the WebView has no find bar of its own, so nothing is displaced. Ctrl replaces ⌘ off macOS. Handled in the WebView (`keydown` on `window`); a native menu bar with accelerators is a later milestone.
9. **Layout and theme (approved mockup):** dark only (`color-scheme: dark`, window `theme: "Dark"` so the native material is dark too); accent `#d91c48`. Grid of a 240 px sidebar and the content column. Sidebar: Search then Home as nav items, the active one a pink pill with white text; an account block at the bottom (photo, name, handle, Sign out). Content: a 56 px sticky header (title and actions, or the search field) that the content scrolls under; the player bar floats inside the content column, inset 24 px from its left, right, and bottom edges, rounded, with transport controls left, artwork + title + progress in the middle, and shuffle/repeat/queue right; the queue button opens an "Up next" list. The content's bottom padding follows the bar's measured height (`--player-space`, set from a `ResizeObserver`), not a fixed number. Home: the first all-song shelf (YouTube Music's Quick picks) is a three-column grid of 44 px-thumbnail rows; other shelves are six-column carousels with previous/next buttons that page by the visible width; album/playlist/song cards are square, artist cards round. Search: filter chips **All, Songs, Albums, Artists, Playlists** (the mockup's 전체/노래/앨범/아티스트/플레이리스트 in the app's current UI language, English; localization is later work) filter the loaded results without a new request; "All" shows a top-result card (the first result; a Play button when it is playable) and then the remaining results grouped by type. Sign-in: an expired session shows a `role="alert"` box above the sign-in button.
10. **Blur (CSS):** the sticky header, the floating player bar, the suggestions dropdown, and the queue panel use `backdrop-filter: saturate(160%) blur(20px)` with the `-webkit-` prefix over a translucent background (popups keep 92 % opacity so text stays legible over busy content).
11. **Native vibrancy:** the main window is `transparent: true` with `theme: "Dark"`. At startup (main thread) `window_effects::apply` calls `window_vibrancy::apply_vibrancy(Sidebar, FollowsWindowActiveState)` on macOS or `apply_mica(dark)` on Windows; `UnsupportedPlatformVersion` (Windows 10) or any error means no effect. The result is managed state read by the `window_vibrancy` command; `main.tsx` asks before the first render and sets `data-vibrancy="native" | "none"` on `<html>`. With `native`, only the sidebar is translucent (a 35 % tint over the material) and every other surface paints `--bg`; with `none` (Windows 10, other platforms, or the browser during tests), the sidebar is the solid `#18181b` and the page paints its background, so the transparent window never shows through. Tauri's config-level `windowEffects` is not used because it cannot report failure and on Windows applies only the first listed effect without fallback (`tauri/src/vibrancy/windows.rs`). macOS Liquid Glass (`LiquidGlassRegular`) is available in this Tauri version but is left for later.
12. **Account block:** `get_account` reads `account/accounts_list` and returns the selected account's name, handle, and photo (`None` if none is named). The Swift app has no Premium detection (`AccountsListParser` and `UserAccount` carry no subscription field), so the block shows name and handle; a "Premium" label needs a verified signal from a live response first (Task 4 Step 8 asks the human to look for one). A failed account request shows "Signed in" instead of an alert.
13. **Player bar fixes:** a seek drag is dropped on `pointercancel` and `blur`; after a committed seek the slider shows the target until the page reports a position within 1.5 s of it or 2 s pass (no snap-back flicker); errors go to the notice area; a failed `playerSnapshot` is reported.
14. **App icon:** rendered from the Swift app's Icon Composer layer (`Sources/Kaset/Resources/AppIcon.icon/Assets/Image Layer.png`) on the macOS icon grid by `app/scripts/render-icon.swift` (AppKit, macOS only) into `app/src-tauri/app-icon.png` (1024×1024), then `pnpm tauri icon` regenerates the bundle icons. The Liquid Glass rendering of the `.icon` file is not reproduced; `kaset.icns` was not used because its largest image is 256 px.

## Items deferred from the M2/M3 reviews

From `.superpowers/sdd/2026-11-22-tauri-port-m2/progress.md` ("Task 7: minor (deferred, M4)") and `.superpowers/sdd/2026-11-22-tauri-port-m3/progress.md` ("Task 6: minor (deferred, M4 UI shell)"):

| Deferred item | Task |
|---|---|
| Tests for `auth_expired` routing and unmounting listeners | 8 (`App.test.tsx`, `useAuthStatus.test.tsx`, `PlayerBar.test.tsx`) |
| HomeScreen retry `setState` without a mounted guard | 8 (TanStack Query owns the request state; no manual `setState`) |
| Sign-out error replaces the home feed | 8 (sign out lives in the sidebar and reports to the notice area) |
| `listen` vs `authStatus` ordering gap | 8 (`useAuthStatus` awaits the subscription first) |
| `authStatus` failure silently becomes `signed_out` | 8 (error screen with "Try again") |
| Seek drag not cleared on `pointercancel` / blur | 8 |
| Optimistic seek flicker | 8 |
| Duplicate error alerts (bar vs home) | 6, 8 (single notice area) |
| Swallowed snapshot failure | 8 |
| Test gaps: unlisten, event-before-snapshot, keyboard seek, auth expiry while mounted | 8 |
| Fixed bottom padding | 8 (padding follows the measured player height) |
| Real app icon instead of the spike placeholder | 9 |

Not in M4 (kept deferred by the M3 rulings): engine polish after the human runtime check (pause during Loading, Failed auto-skip, load generation, previous/seek in Ended, `close()`→`destroy()`, player allowlist narrowing).

## File Structure

```
crates/ytm-core/
  src/models.rs                       # + SearchItem, SearchResults (T2), Account (T4)
  src/client.rs                       # + search, search_suggestions (T3), accounts_list (T4)
  src/parsers/mod.rs                  # + search (T2), search_suggestions (T3), account (T4)
  src/parsers/helpers.rs              # is_clock_duration → pub(super)
  src/parsers/home.rs                 # + musicShelf, musicCardShelf, immersive carousel, grid, itemSection
  src/parsers/search.rs               # NEW port of SearchResponseParser (first page)
  src/parsers/search_suggestions.rs   # NEW port of SearchSuggestionsParser
  src/parsers/account.rs              # NEW selected account (name, handle, photo)
  tests/home_parser.rs                # + shelf-type tests
  tests/search_parser.rs, search_suggestions_parser.rs, account_parser.rs   # NEW
  tests/client.rs                     # + search / suggestions / accounts_list requests
  tests/fixtures/home_shelf_types.json            # NEW (synthetic, placeholder ids)
  tests/fixtures/search_*.json                    # COPIED from Tests/KasetTests/Fixtures (6 files)
  tests/snapshots/home_parser__parses_every_supported_shelf_type.snap   # NEW
  tests/snapshots/search_parser__parses_shelves_in_server_order.snap    # NEW
crates/ytm-explorer/src/main.rs       # + search, suggest (T3), account (T4)
app/src-tauri/
  Cargo.toml, ../../Cargo.lock        # + window-vibrancy 0.8.1 (already locked)
  src/commands.rs                     # signed_in_request; get_home, search, search_suggestions, get_account, window_vibrancy
  src/window_effects.rs               # NEW macOS vibrancy / Windows 11 Mica
  src/lib.rs, build.rs                # register commands, apply effects in setup
  capabilities/default.json           # + four permissions
  permissions/autogenerated/{search,search_suggestions,get_account,window_vibrancy}.toml   # generated
  tauri.conf.json                     # main window theme Dark + transparent; img-src https://*.ytimg.com
  app-icon.png, icons/*               # Task 9
app/scripts/render-icon.swift         # NEW (Task 9)
app/package.json, pnpm-lock.yaml      # + @tanstack/react-query 5.107.1
app/src/
  bindings.ts                         # regenerated (Task 5)
  main.tsx, App.tsx, App.test.tsx     # vibrancy flag, query client, auth gate, shell
  query.ts (+test), errors.ts         # query client, retry policy, keys
  notices/NoticeProvider.tsx (+test)  # single notice area
  music/items.ts, imageUrl.ts (+test), Artwork.tsx (+test), ItemCard.tsx
  auth/useAuthStatus.ts (+test), auth/SignInScreen.tsx (+test)
  shell/Shell.tsx, Sidebar.tsx, shortcuts.ts (+test), useShortcuts.ts
  home/HomeScreen.tsx (+test)
  search/SearchScreen.tsx (+test), SearchBox.tsx, groups.ts, useDebouncedValue.ts
  player/PlayerBar.tsx (+test), usePlayer.ts, usePlayQueue.ts
  test/setup.ts, test/render.tsx, test/fixtures.ts
  styles.css
docs/keyboard-shortcuts.md            # Tauri app section (Task 8)
docs/adr/0045-tauri-ui-shell.md       # NEW (Task 10)
AGENTS.md                             # Tauri section (Task 10)
```

Task order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10. Tasks 2–4 share `models.rs`, `parsers/mod.rs`, `client.rs`, and the explorer; Task 5 needs Tasks 2–4; Task 7 needs Tasks 5–6 (bindings, building blocks); Task 8 needs Tasks 6–7; Task 9 is independent of 6–8 but runs after them to keep CI builds focused.

---
### Task 1: Home parser — every shelf type the Swift parser supports

The M1 parser reads only `musicCarouselShelfRenderer`. The Swift `HomeResponseParser.parseHomeSection` also reads `musicShelfRenderer` (vertical lists, title in `title`), `musicCardShelfRenderer` (title in `header.musicCardShelfHeaderBasicRenderer.title`, default "Featured"), `musicImmersiveCarouselShelfRenderer` (carousel header, default "Featured"), `gridRenderer` (items in `items`, title in `header.gridHeaderRenderer.title`, default "Charts"), and `itemSectionRenderer` wrapping one of those (first one that parses). Mood/genre buttons (`musicNavigationButtonRenderer`) stay unsupported, so a grid of only buttons is skipped.

**Files:**
- Modify: `crates/ytm-core/src/parsers/home.rs` (`parse_section` and new helpers)
- Create: `crates/ytm-core/tests/fixtures/home_shelf_types.json`, `crates/ytm-core/tests/snapshots/home_parser__parses_every_supported_shelf_type.snap`
- Test: `crates/ytm-core/tests/home_parser.rs` (append)

**Interfaces:**
- Consumes: existing `parse_home(&Value) -> Result<HomePage, YtmError>`, `parse_item`, `json::{at, runs_text}`.
- Produces: same public API; `parse_home` now returns sections for the five extra shelf types. Private helpers `carousel_title(&Value) -> Option<String>`, `shelf_section(String, Option<&Value>) -> Option<HomeSection>`, `first_key(&Value) -> &str`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ytm-core/tests/fixtures/home_shelf_types.json`:

```json
{
  "contents": {
    "singleColumnBrowseResultsRenderer": {
      "tabs": [
        {
          "tabRenderer": {
            "content": {
              "sectionListRenderer": {
                "contents": [
                  {
                    "musicShelfRenderer": {
                      "title": {
                        "runs": [
                          {
                            "text": "Top songs"
                          }
                        ]
                      },
                      "contents": [
                        {
                          "musicResponsiveListItemRenderer": {
                            "playlistItemData": {
                              "videoId": "test-vid-01"
                            },
                            "flexColumns": [
                              {
                                "musicResponsiveListItemFlexColumnRenderer": {
                                  "text": {
                                    "runs": [
                                      {
                                        "text": "Shelf song"
                                      }
                                    ]
                                  }
                                }
                              },
                              {
                                "musicResponsiveListItemFlexColumnRenderer": {
                                  "text": {
                                    "runs": [
                                      {
                                        "text": "Shelf artist",
                                        "navigationEndpoint": {
                                          "browseEndpoint": {
                                            "browseId": "UCshelf-artist"
                                          }
                                        }
                                      }
                                    ]
                                  }
                                }
                              }
                            ],
                            "thumbnail": {
                              "musicThumbnailRenderer": {
                                "thumbnail": {
                                  "thumbnails": [
                                    {
                                      "url": "https://lh3.googleusercontent.com/test-song=w120-h120",
                                      "width": 120,
                                      "height": 120
                                    }
                                  ]
                                }
                              }
                            }
                          }
                        }
                      ]
                    }
                  },
                  {
                    "musicCardShelfRenderer": {
                      "header": {
                        "musicCardShelfHeaderBasicRenderer": {
                          "title": {
                            "runs": [
                              {
                                "text": "Mixed for you"
                              }
                            ]
                          }
                        }
                      },
                      "contents": [
                        {
                          "musicTwoRowItemRenderer": {
                            "title": {
                              "runs": [
                                {
                                  "text": "Card playlist"
                                }
                              ]
                            },
                            "subtitle": {
                              "runs": [
                                {
                                  "text": "Playlist • Test curator"
                                }
                              ]
                            },
                            "navigationEndpoint": {
                              "browseEndpoint": {
                                "browseId": "VLtest-playlist",
                                "browseEndpointContextSupportedConfigs": {
                                  "browseEndpointContextMusicConfig": {
                                    "pageType": "MUSIC_PAGE_TYPE_PLAYLIST"
                                  }
                                }
                              }
                            },
                            "thumbnail": {
                              "musicThumbnailRenderer": {
                                "thumbnail": {
                                  "thumbnails": [
                                    {
                                      "url": "https://i.ytimg.com/vi/test-vid-02/sddefault.jpg",
                                      "width": 120,
                                      "height": 120
                                    }
                                  ]
                                }
                              }
                            }
                          }
                        }
                      ]
                    }
                  },
                  {
                    "musicImmersiveCarouselShelfRenderer": {
                      "contents": [
                        {
                          "musicTwoRowItemRenderer": {
                            "title": {
                              "runs": [
                                {
                                  "text": "Immersive album"
                                }
                              ]
                            },
                            "subtitle": {
                              "runs": [
                                {
                                  "text": "Album • Test artist"
                                }
                              ]
                            },
                            "navigationEndpoint": {
                              "browseEndpoint": {
                                "browseId": "MPREtest-album",
                                "browseEndpointContextSupportedConfigs": {
                                  "browseEndpointContextMusicConfig": {
                                    "pageType": "MUSIC_PAGE_TYPE_ALBUM"
                                  }
                                }
                              }
                            },
                            "thumbnail": {
                              "musicThumbnailRenderer": {
                                "thumbnail": {
                                  "thumbnails": [
                                    {
                                      "url": "https://lh3.googleusercontent.com/test-album=w226-h226",
                                      "width": 120,
                                      "height": 120
                                    }
                                  ]
                                }
                              }
                            }
                          }
                        }
                      ]
                    }
                  },
                  {
                    "gridRenderer": {
                      "header": {
                        "gridHeaderRenderer": {
                          "title": {
                            "runs": [
                              {
                                "text": "Top artists"
                              }
                            ]
                          }
                        }
                      },
                      "items": [
                        {
                          "musicTwoRowItemRenderer": {
                            "title": {
                              "runs": [
                                {
                                  "text": "Grid artist"
                                }
                              ]
                            },
                            "subtitle": {
                              "runs": [
                                {
                                  "text": "1.2M subscribers"
                                }
                              ]
                            },
                            "navigationEndpoint": {
                              "browseEndpoint": {
                                "browseId": "UCgrid-artist",
                                "browseEndpointContextSupportedConfigs": {
                                  "browseEndpointContextMusicConfig": {
                                    "pageType": "MUSIC_PAGE_TYPE_ARTIST"
                                  }
                                }
                              }
                            },
                            "thumbnail": {
                              "musicThumbnailRenderer": {
                                "thumbnail": {
                                  "thumbnails": [
                                    {
                                      "url": "https://yt3.ggpht.com/test-artist=w226-h226",
                                      "width": 120,
                                      "height": 120
                                    }
                                  ]
                                }
                              }
                            }
                          }
                        }
                      ]
                    }
                  },
                  {
                    "itemSectionRenderer": {
                      "contents": [
                        {
                          "musicCarouselShelfRenderer": {
                            "header": {
                              "musicCarouselShelfBasicHeaderRenderer": {
                                "title": {
                                  "runs": [
                                    {
                                      "text": "Wrapped carousel"
                                    }
                                  ]
                                }
                              }
                            },
                            "contents": [
                              {
                                "musicTwoRowItemRenderer": {
                                  "title": {
                                    "runs": [
                                      {
                                        "text": "Wrapped song"
                                      }
                                    ]
                                  },
                                  "subtitle": {
                                    "runs": [
                                      {
                                        "text": "Wrapped artist",
                                        "navigationEndpoint": {
                                          "browseEndpoint": {
                                            "browseId": "UCwrapped-artist"
                                          }
                                        }
                                      }
                                    ]
                                  },
                                  "navigationEndpoint": {
                                    "watchEndpoint": {
                                      "videoId": "test-vid-03"
                                    }
                                  }
                                }
                              }
                            ]
                          }
                        }
                      ]
                    }
                  },
                  {
                    "gridRenderer": {
                      "items": [
                        {
                          "musicNavigationButtonRenderer": {
                            "buttonText": {
                              "runs": [
                                {
                                  "text": "Chill"
                                }
                              ]
                            },
                            "clickCommand": {
                              "browseEndpoint": {
                                "browseId": "FEmusic_moods_and_genres_category"
                              }
                            }
                          }
                        }
                      ]
                    }
                  },
                  {
                    "musicTastebuilderShelfRenderer": {}
                  }
                ],
                "continuations": [
                  {
                    "nextContinuationData": {
                      "continuation": "test-home-continuation"
                    }
                  }
                ]
              }
            }
          }
        }
      ]
    }
  }
}
```

Create `crates/ytm-core/tests/snapshots/home_parser__parses_every_supported_shelf_type.snap` (insta compares against it; do not run with `INSTA_UPDATE=always`):

```yaml
---
source: crates/ytm-core/tests/home_parser.rs
expression: page
---
sections:
  - title: Top songs
    items:
      - kind: song
        video_id: test-vid-01
        title: Shelf song
        artists:
          - name: Shelf artist
            browse_id: UCshelf-artist
        thumbnail_url: "https://lh3.googleusercontent.com/test-song=w120-h120"
  - title: Mixed for you
    items:
      - kind: playlist
        browse_id: VLtest-playlist
        title: Card playlist
        subtitle: Playlist • Test curator
        thumbnail_url: "https://i.ytimg.com/vi/test-vid-02/sddefault.jpg"
  - title: Featured
    items:
      - kind: album
        browse_id: MPREtest-album
        title: Immersive album
        subtitle: Album • Test artist
        thumbnail_url: "https://lh3.googleusercontent.com/test-album=w226-h226"
  - title: Top artists
    items:
      - kind: artist
        browse_id: UCgrid-artist
        title: Grid artist
        subtitle: 1.2M subscribers
        thumbnail_url: "https://yt3.ggpht.com/test-artist=w226-h226"
  - title: Wrapped carousel
    items:
      - kind: song
        video_id: test-vid-03
        title: Wrapped song
        artists:
          - name: Wrapped artist
            browse_id: UCwrapped-artist
        thumbnail_url: ~
continuation: test-home-continuation
```

Append to the end of `crates/ytm-core/tests/home_parser.rs`:

```rust
#[test]
fn parses_every_supported_shelf_type() {
    let response: Value =
        serde_json::from_str(include_str!("fixtures/home_shelf_types.json")).unwrap();
    let page = parse_home(&response).unwrap();

    let titles: Vec<&str> = page.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Top songs",
            "Mixed for you",
            "Featured",
            "Top artists",
            "Wrapped carousel"
        ]
    );
    assert_eq!(page.continuation.as_deref(), Some("test-home-continuation"));
    assert!(
        matches!(&page.sections[0].items[0], ShelfItem::Song(song) if song.video_id == "test-vid-01")
    );
    assert!(
        matches!(&page.sections[1].items[0], ShelfItem::Playlist(card) if card.browse_id == "VLtest-playlist")
    );
    assert!(
        matches!(&page.sections[2].items[0], ShelfItem::Album(card) if card.browse_id == "MPREtest-album")
    );
    assert!(
        matches!(&page.sections[3].items[0], ShelfItem::Artist(card) if card.browse_id == "UCgrid-artist")
    );
    assert!(
        matches!(&page.sections[4].items[0], ShelfItem::Song(song) if song.video_id == "test-vid-03")
    );

    insta::assert_yaml_snapshot!(page);
}

#[test]
fn item_section_without_a_supported_shelf_is_skipped() {
    let response = json!({ "contents": { "singleColumnBrowseResultsRenderer": { "tabs": [ { "tabRenderer": {
        "content": { "sectionListRenderer": { "contents": [
            { "itemSectionRenderer": { "contents": [ { "messageRenderer": {} } ] } }
        ] } } } } ] } } });
    assert!(parse_home(&response).unwrap().sections.is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ytm-core --test home_parser`
Expected: FAIL in `parses_every_supported_shelf_type` (the titles assertion sees only the sections the carousel-only parser finds).

- [ ] **Step 3: Implement the shelf types**

Replace the whole of `crates/ytm-core/src/parsers/home.rs` with:

```rust
use serde_json::Value;

use super::helpers::{
    artists_from_flex_column, artists_from_runs, is_artist_page_type, is_navigable_artist_id,
    page_type,
};
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{BrowseCard, HomePage, HomeSection, ShelfItem, Song};

const SECTION_LIST: &[&str] = &[
    "contents",
    "singleColumnBrowseResultsRenderer",
    "tabs",
    "0",
    "tabRenderer",
    "content",
    "sectionListRenderer",
];

const PLAY_BUTTON_VIDEO_ID: &[&str] = &[
    "overlay",
    "musicItemThumbnailOverlayRenderer",
    "content",
    "musicPlayButtonRenderer",
    "playNavigationEndpoint",
    "watchEndpoint",
    "videoId",
];

pub fn parse_home(response: &Value) -> Result<HomePage, YtmError> {
    let list = at(response, SECTION_LIST).ok_or_else(|| YtmError::Parse {
        endpoint: "browse:FEmusic_home".into(),
        path: SECTION_LIST.join("."),
    })?;
    let sections = list
        .get("contents")
        .and_then(Value::as_array)
        .map(|contents| contents.iter().filter_map(parse_section).collect())
        .unwrap_or_default();
    let continuation = str_at(
        list,
        &["continuations", "0", "nextContinuationData", "continuation"],
    )
    .map(str::to_owned);
    Ok(HomePage {
        sections,
        continuation,
    })
}

/// One home section. Shelf types follow the Swift `HomeResponseParser`:
/// carousels, vertical shelves, card shelves, immersive carousels, grids, and
/// item sections that wrap one of them. Mood and genre buttons are skipped.
fn parse_section(section: &Value) -> Option<HomeSection> {
    if let Some(shelf) = section.get("musicCarouselShelfRenderer") {
        let title = carousel_title(shelf).unwrap_or_else(|| "Unknown Section".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section.get("musicShelfRenderer") {
        let title = shelf
            .get("title")
            .and_then(runs_text)
            .unwrap_or_else(|| "Unknown Section".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section.get("musicCardShelfRenderer") {
        let title = at(
            shelf,
            &["header", "musicCardShelfHeaderBasicRenderer", "title"],
        )
        .and_then(runs_text)
        .unwrap_or_else(|| "Featured".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section.get("musicImmersiveCarouselShelfRenderer") {
        let title = carousel_title(shelf).unwrap_or_else(|| "Featured".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(grid) = section.get("gridRenderer") {
        let title = at(grid, &["header", "gridHeaderRenderer", "title"])
            .and_then(runs_text)
            .unwrap_or_else(|| "Charts".into());
        return shelf_section(title, grid.get("items"));
    }
    if let Some(wrapper) = section.get("itemSectionRenderer") {
        return wrapper
            .get("contents")?
            .as_array()?
            .iter()
            .find_map(parse_section);
    }
    tracing::debug!(
        kind = first_key(section),
        "skipping unsupported home section"
    );
    None
}

fn carousel_title(shelf: &Value) -> Option<String> {
    at(
        shelf,
        &["header", "musicCarouselShelfBasicHeaderRenderer", "title"],
    )
    .and_then(runs_text)
}

/// A section from `contents`; `None` when no item parses.
fn shelf_section(title: String, contents: Option<&Value>) -> Option<HomeSection> {
    let items: Vec<ShelfItem> = contents?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let parsed = parse_item(item);
            if parsed.is_none() {
                tracing::debug!(kind = first_key(item), "skipping unparsed home shelf item");
            }
            parsed
        })
        .collect();
    (!items.is_empty()).then_some(HomeSection { title, items })
}

fn first_key(value: &Value) -> &str {
    value
        .as_object()
        .and_then(|object| object.keys().next())
        .map_or("?", String::as_str)
}

pub(crate) fn parse_item(item: &Value) -> Option<ShelfItem> {
    if let Some(renderer) = item.get("musicTwoRowItemRenderer") {
        return parse_two_row(renderer);
    }
    if let Some(renderer) = item.get("musicResponsiveListItemRenderer") {
        return parse_responsive(renderer);
    }
    None
}

fn parse_two_row(renderer: &Value) -> Option<ShelfItem> {
    let title = renderer.get("title").and_then(runs_text)?;
    let thumbnail = thumbnail_url(renderer);
    let nav = renderer.get("navigationEndpoint")?;
    if let Some(video_id) = str_at(nav, &["watchEndpoint", "videoId"]) {
        return Some(ShelfItem::Song(Song {
            video_id: video_id.into(),
            title,
            artists: artists_from_runs(renderer.get("subtitle")),
            thumbnail_url: thumbnail,
        }));
    }
    let browse = nav.get("browseEndpoint")?;
    let card = BrowseCard {
        browse_id: browse.get("browseId")?.as_str()?.into(),
        title,
        subtitle: renderer.get("subtitle").and_then(runs_text),
        thumbnail_url: thumbnail,
    };
    classify(page_type(browse), card)
}

fn parse_responsive(renderer: &Value) -> Option<ShelfItem> {
    let title = flex_column(renderer, 0).and_then(runs_text)?;
    let thumbnail = thumbnail_url(renderer);
    let video_id = str_at(renderer, &["playlistItemData", "videoId"])
        .or_else(|| {
            str_at(
                renderer,
                &["navigationEndpoint", "watchEndpoint", "videoId"],
            )
        })
        .or_else(|| str_at(renderer, PLAY_BUTTON_VIDEO_ID));
    if let Some(video_id) = video_id {
        return Some(ShelfItem::Song(Song {
            video_id: video_id.into(),
            title,
            artists: artists_from_flex_column(flex_column(renderer, 1)),
            thumbnail_url: thumbnail,
        }));
    }
    let browse = at(renderer, &["navigationEndpoint", "browseEndpoint"])?;
    let card = BrowseCard {
        browse_id: browse.get("browseId")?.as_str()?.into(),
        title,
        subtitle: flex_column(renderer, 1).and_then(runs_text),
        thumbnail_url: thumbnail,
    };
    classify(page_type(browse), card)
}

fn flex_column(renderer: &Value, index: usize) -> Option<&Value> {
    renderer
        .get("flexColumns")?
        .get(index)?
        .get("musicResponsiveListItemFlexColumnRenderer")?
        .get("text")
}

/// pageType first, then browseId prefix — same order as the Swift HomeResponseParser.
fn classify(page_type: Option<&str>, card: BrowseCard) -> Option<ShelfItem> {
    match page_type {
        Some("MUSIC_PAGE_TYPE_ALBUM") => return Some(ShelfItem::Album(card)),
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => return Some(ShelfItem::Playlist(card)),
        page_type if is_artist_page_type(page_type) => {
            return Some(ShelfItem::Artist(card));
        }
        _ => {}
    }
    let id = card.browse_id.as_str();
    if id.starts_with("MPRE") || id.starts_with("OLAK") {
        Some(ShelfItem::Album(card))
    } else if ["VL", "PL", "RD"].iter().any(|p| id.starts_with(p)) {
        Some(ShelfItem::Playlist(card))
    } else if is_navigable_artist_id(id) {
        Some(ShelfItem::Artist(card))
    } else {
        None
    }
}
```

- [ ] **Step 4: Run the checks**

Run: `cargo fmt --all --check && cargo clippy -p ytm-core --all-targets -- -D warnings && cargo test -p ytm-core`
Expected: `home_parser` reports `11 passed`; every other ytm-core test binary still passes (lib 16, api_key 4, client 8).

- [ ] **Step 5: Commit and push**

```bash
git add crates/ytm-core/src/parsers/home.rs crates/ytm-core/tests/home_parser.rs crates/ytm-core/tests/fixtures/home_shelf_types.json crates/ytm-core/tests/snapshots/home_parser__parses_every_supported_shelf_type.snap
git commit -m "feat(core): parse shelf, card, immersive, grid, and item-section home shelves"
git push origin main
```

---

### Task 2: Search parser — port of the Swift `SearchResponseParser`

Ports the first-page path of `Sources/Kaset/Services/API/Parsers/SearchResponseParser.swift` and `SearchResponseParser+Support.swift`: section lists from `contents.sectionListRenderer` or every `tabbedSearchResultsRenderer` tab; containers `musicCardShelfRenderer` (the card itself is the top result, then its contents), `musicShelfRenderer`, `itemSectionRenderer`, and items `musicResponsiveListItemRenderer` / `musicTwoRowItemRenderer` / `musicMultiRowListItemRenderer`; endpoint candidates per renderer type (unwrapping `innertubeCommand`); browse kind by `pageType` first, then browse id prefix; playlist ids in `watchPlaylistEndpoint` (and `watchEndpoint.playlistId` for playlist-typed results); playable items by `playlistItemData.videoId` then endpoint `watchEndpoint.videoId`; video vs song by content label or `musicVideoType` (OMV, UGC, OFFICIAL_SOURCE_MUSIC are videos); dedupe by identity (`video:`, `album:`, `artist:`, `playlist:`, `podcast:`), first occurrence wins; first non-empty continuation token. Episode metadata (dates, durations, progress) and audiobook/album distinction are not ported (Decision 1).

**Files:**
- Modify: `crates/ytm-core/src/models.rs`, `crates/ytm-core/src/parsers/helpers.rs`, `crates/ytm-core/src/parsers/mod.rs`
- Create: `crates/ytm-core/src/parsers/search.rs`, `crates/ytm-core/tests/search_parser.rs`, `crates/ytm-core/tests/snapshots/search_parser__parses_shelves_in_server_order.snap`
- Copy: `Tests/KasetTests/Fixtures/{search_response,search_direct_item_section,search_tabbed_mixed,search_top_result_watch_endpoints,search_playlist_watch_endpoints,search_later_flex_columns}.json` → `crates/ytm-core/tests/fixtures/`

**Interfaces:**
- Consumes: `models::{Song, BrowseCard, ArtistRef}`; `parsers::helpers::{artists_from_flex_column, is_artist_page_type, is_navigable_artist_id, page_type, is_clock_duration}`; `json::{at, str_at, thumbnail_url}`.
- Produces: `ytm_core::models::SearchItem` (`#[serde(tag = "kind", rename_all = "snake_case")]`: `Song(Song) | Video(Song) | Episode(Song) | Album(BrowseCard) | Playlist(BrowseCard) | Artist(BrowseCard) | Profile(BrowseCard) | Podcast(BrowseCard)`), `ytm_core::models::SearchResults { items: Vec<SearchItem>, continuation: Option<String> }` (both `Serialize`, `specta::Type` behind the `specta` feature), and `ytm_core::parsers::search::parse_search(&Value) -> SearchResults` (never fails; a response without a section list is empty).

- [ ] **Step 1: Copy the fixtures**

```bash
for name in search_response search_direct_item_section search_tabbed_mixed search_top_result_watch_endpoints search_playlist_watch_endpoints search_later_flex_columns; do
  cp "Tests/KasetTests/Fixtures/$name.json" crates/ytm-core/tests/fixtures/
done
git status --short Tests/   # must print nothing: the Swift fixtures stay in place
```

- [ ] **Step 2: Write the failing tests**

Create `crates/ytm-core/tests/search_parser.rs`:

```rust
use serde_json::{Value, json};
use ytm_core::models::{ArtistRef, SearchItem};
use ytm_core::parsers::search::parse_search;

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// `kind:id` for each item, like the Swift tests' `allItems.map(\.id)`.
fn ids(items: &[SearchItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            SearchItem::Song(song) => format!("song:{}", song.video_id),
            SearchItem::Video(song) => format!("video:{}", song.video_id),
            SearchItem::Episode(song) => format!("episode:{}", song.video_id),
            SearchItem::Album(card) => format!("album:{}", card.browse_id),
            SearchItem::Playlist(card) => format!("playlist:{}", card.browse_id),
            SearchItem::Artist(card) => format!("artist:{}", card.browse_id),
            SearchItem::Profile(card) => format!("profile:{}", card.browse_id),
            SearchItem::Podcast(card) => format!("podcast:{}", card.browse_id),
        })
        .collect()
}

#[test]
fn parses_shelves_in_server_order() {
    let results = parse_search(&fixture("search_response"));
    assert_eq!(
        ids(&results.items),
        [
            "song:search-video-1",
            "song:search-video-2",
            "album:MPREb_search_album_1",
            "artist:UC-search-result-artist",
        ]
    );
    let SearchItem::Song(song) = &results.items[0] else {
        panic!("expected a song");
    };
    assert_eq!(song.title, "Search Result Song 1");
    assert_eq!(
        song.artists,
        [ArtistRef {
            name: "Search Artist 1".into(),
            browse_id: Some("UC-search-artist-1".into()),
        }]
    );
    assert_eq!(
        song.thumbnail_url.as_deref(),
        Some("https://example.com/search-thumb-1.jpg")
    );
    assert_eq!(results.continuation, None);
    insta::assert_yaml_snapshot!(results);
}

#[test]
fn direct_item_section_roots_keep_server_order() {
    let results = parse_search(&fixture("search_direct_item_section"));
    assert_eq!(
        ids(&results.items),
        ["song:direct-song-video", "album:MPREdirect-album"]
    );
}

#[test]
fn tabbed_mixed_results_keep_order_and_first_occurrence() {
    let results = parse_search(&fixture("search_tabbed_mixed"));
    assert_eq!(
        ids(&results.items),
        [
            "profile:UCtop-profile",
            "video:official-source-video",
            "podcast:MPSPPfixture-show",
            "episode:fixture-episode-video",
            "playlist:VLfixture-playlist",
        ]
    );
    let SearchItem::Profile(profile) = &results.items[0] else {
        panic!("expected a profile");
    };
    assert_eq!(profile.subtitle.as_deref(), Some("42K subscribers"));
    let SearchItem::Video(video) = &results.items[1] else {
        panic!("expected a video");
    };
    assert_eq!(video.title, "Official Source Video");
    let SearchItem::Episode(episode) = &results.items[3] else {
        panic!("expected an episode");
    };
    assert_eq!(
        episode.artists,
        [ArtistRef {
            name: "Fixture Podcast".into(),
            browse_id: Some("MPSPPfixture-show".into()),
        }]
    );
    assert_eq!(
        results.continuation.as_deref(),
        Some("sanitized-next-page-token")
    );
}

#[test]
fn top_result_watch_destinations_parse_from_every_endpoint_source() {
    let results = parse_search(&fixture("search_top_result_watch_endpoints"));
    assert_eq!(
        ids(&results.items),
        [
            "video:title-endpoint-video",
            "video:on-tap-video",
            "video:thumbnail-overlay-video",
        ]
    );
}

#[test]
fn playlist_ids_in_watch_endpoints_stay_playlists() {
    let results = parse_search(&fixture("search_playlist_watch_endpoints"));
    let playlists: Vec<_> = ids(&results.items)
        .into_iter()
        .filter(|id| !id.starts_with("album:"))
        .collect();
    assert_eq!(
        playlists,
        ["playlist:RDradio-watch", "playlist:VMplaylist-action"]
    );
    assert!(ids(&results.items).contains(&"album:MPREcanonical-album".to_owned()));
}

#[test]
fn later_flex_columns_contribute_metadata() {
    let results = parse_search(&fixture("search_later_flex_columns"));
    let Some(SearchItem::Album(album)) = results.items.first() else {
        panic!("expected an album, got {:?}", results.items);
    };
    assert!(
        album
            .subtitle
            .as_deref()
            .unwrap_or_default()
            .contains("2026"),
        "{album:?}"
    );
}

#[test]
fn missing_section_list_is_an_empty_result() {
    let results = parse_search(&json!({ "contents": {} }));
    assert!(results.items.is_empty());
    assert_eq!(results.continuation, None);
}

fn shelf(items: Value) -> Value {
    json!({ "contents": { "sectionListRenderer": { "contents": [
        { "musicShelfRenderer": { "contents": items } }
    ] } } })
}

#[test]
fn untyped_video_cards_use_the_plain_creator_name() {
    let results = parse_search(&shelf(json!([{ "musicMultiRowListItemRenderer": {
        "title": { "runs": [{ "text": "Creator video" }] },
        "subtitle": { "runs": [{ "text": "Video • Fixture Creator • 1.2M views" }] },
        "onTap": { "watchEndpoint": { "videoId": "creator-video" } }
    } }])));
    let [SearchItem::Video(video)] = results.items.as_slice() else {
        panic!("expected one video, got {:?}", results.items);
    };
    assert_eq!(
        video.artists,
        [ArtistRef {
            name: "Fixture Creator".into(),
            browse_id: None,
        }]
    );
}

#[test]
fn count_only_subtitles_do_not_become_artists() {
    let results = parse_search(&shelf(json!([{ "musicResponsiveListItemRenderer": {
        "playlistItemData": { "videoId": "count-song" },
        "flexColumns": [
            { "musicResponsiveListItemFlexColumnRenderer": {
                "text": { "runs": [{ "text": "Counted" }] } } },
            { "musicResponsiveListItemFlexColumnRenderer": {
                "text": { "runs": [{ "text": "Song" }, { "text": " • " }, { "text": "12 songs" }] } } }
        ]
    } }])));
    let [SearchItem::Song(song)] = results.items.as_slice() else {
        panic!("expected one song, got {:?}", results.items);
    };
    assert!(song.artists.is_empty(), "{song:?}");
}

#[test]
fn library_artist_page_type_is_an_artist() {
    let results = parse_search(&shelf(json!([{ "musicResponsiveListItemRenderer": {
        "navigationEndpoint": { "browseEndpoint": {
            "browseId": "MPLAUC1234567890",
            "browseEndpointContextSupportedConfigs": {
                "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_LIBRARY_ARTIST" }
            }
        } },
        "flexColumns": [{ "musicResponsiveListItemFlexColumnRenderer": {
            "text": { "runs": [{ "text": "Library Artist" }] } } }]
    } }])));
    assert_eq!(ids(&results.items), ["artist:MPLAUC1234567890"]);
}

#[test]
fn shelf_continuation_items_carry_the_next_token() {
    let results = parse_search(&shelf(json!([
        { "continuationItemRenderer": { "continuationEndpoint": {
            "continuationCommand": { "token": "sanitized-item-token" } } } }
    ])));
    assert_eq!(
        results.continuation.as_deref(),
        Some("sanitized-item-token")
    );
}
```

Create `crates/ytm-core/tests/snapshots/search_parser__parses_shelves_in_server_order.snap`:

```yaml
---
source: crates/ytm-core/tests/search_parser.rs
expression: results
---
items:
  - kind: song
    video_id: search-video-1
    title: Search Result Song 1
    artists:
      - name: Search Artist 1
        browse_id: UC-search-artist-1
    thumbnail_url: "https://example.com/search-thumb-1.jpg"
  - kind: song
    video_id: search-video-2
    title: Search Result Song 2
    artists:
      - name: Search Artist 2
        browse_id: UC-search-artist-2
    thumbnail_url: "https://example.com/search-thumb-2.jpg"
  - kind: album
    browse_id: MPREb_search_album_1
    title: Search Album 1
    subtitle: Album Artist
    thumbnail_url: "https://example.com/search-album-1.jpg"
  - kind: artist
    browse_id: UC-search-result-artist
    title: Search Result Artist
    subtitle: ~
    thumbnail_url: "https://example.com/search-artist.jpg"
continuation: ~
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ytm-core --test search_parser`
Expected: FAIL to compile — `unresolved import ytm_core::models::SearchItem` and `ytm_core::parsers::search`.

- [ ] **Step 4: Add the models**

In `crates/ytm-core/src/models.rs`, insert immediately above the line `#[cfg(all(test, feature = "specta"))]`:

```rust
/// One search result, in server order. Songs, videos, and episodes carry a
/// video id and can be queued; the rest open detail pages (milestone 5).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchItem {
    Song(Song),
    Video(Song),
    /// A podcast episode; `artists` holds the show when the result links it.
    Episode(Song),
    /// Albums, singles, EPs, and audiobooks.
    Album(BrowseCard),
    Playlist(BrowseCard),
    Artist(BrowseCard),
    /// A user channel (`MUSIC_PAGE_TYPE_USER_CHANNEL`).
    Profile(BrowseCard),
    Podcast(BrowseCard),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchResults {
    pub items: Vec<SearchItem>,
    /// Token for the next page; not requested yet.
    pub continuation: Option<String>,
}

```

In the same file's `models_implement_specta_type` test, after `assert_type::<HomePage>();` add:

```rust
        assert_type::<SearchItem>();
        assert_type::<SearchResults>();
```

- [ ] **Step 5: Expose the duration helper and register the module**

In `crates/ytm-core/src/parsers/helpers.rs`, change `fn is_clock_duration(text: &str) -> bool {` to:

```rust
pub(super) fn is_clock_duration(text: &str) -> bool {
```

Replace `crates/ytm-core/src/parsers/mod.rs` with:

```rust
mod helpers;
pub mod home;
pub mod search;
```

- [ ] **Step 6: Implement the parser**

Create `crates/ytm-core/src/parsers/search.rs`:

```rust
//! `search` responses. Ported from the Swift `SearchResponseParser`: results
//! keep server order, the first occurrence of each item wins, and browse
//! semantics (album, artist, playlist, …) win over watch fallbacks.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::helpers::{
    artists_from_flex_column, is_artist_page_type, is_clock_duration, is_navigable_artist_id,
    page_type,
};
use crate::json::{at, str_at, thumbnail_url};
use crate::models::{ArtistRef, BrowseCard, SearchItem, SearchResults, Song};

/// Parses the first page of a search. A response without a section list is
/// an empty result, as in the Swift parser (YouTube returns one for queries
/// with no matches).
pub fn parse_search(response: &Value) -> SearchResults {
    let mut results = Accumulator::default();
    let lists = first_page_section_lists(response);
    if lists.is_empty() {
        tracing::debug!("search response has no section list");
    }
    for list in lists {
        for content in array(list.get("contents")) {
            parse_container(content, &mut results);
        }
        results.capture(continuation_token(list));
    }
    SearchResults {
        items: results.items,
        continuation: results.continuation,
    }
}

#[derive(Default)]
struct Accumulator {
    items: Vec<SearchItem>,
    seen: HashSet<String>,
    continuation: Option<String>,
}

impl Accumulator {
    fn push(&mut self, item: SearchItem) {
        if self.seen.insert(identity(&item)) {
            self.items.push(item);
        }
    }

    fn capture(&mut self, token: Option<&str>) {
        if self.continuation.is_some() {
            return;
        }
        if let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) {
            self.continuation = Some(token.to_owned());
        }
    }
}

/// Songs, videos, and episodes share the video namespace, so a video listed
/// twice under different kinds appears once.
fn identity(item: &SearchItem) -> String {
    match item {
        SearchItem::Song(song) | SearchItem::Video(song) | SearchItem::Episode(song) => {
            format!("video:{}", song.video_id)
        }
        SearchItem::Album(card) => format!("album:{}", card.browse_id),
        SearchItem::Artist(card) | SearchItem::Profile(card) => {
            format!("artist:{}", card.browse_id)
        }
        SearchItem::Playlist(card) => format!("playlist:{}", card.browse_id),
        SearchItem::Podcast(card) => format!("podcast:{}", card.browse_id),
    }
}

fn array(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn first_page_section_lists(response: &Value) -> Vec<&Value> {
    let Some(contents) = response.get("contents") else {
        return Vec::new();
    };
    if let Some(list) = contents.get("sectionListRenderer") {
        return vec![list];
    }
    array(at(contents, &["tabbedSearchResultsRenderer", "tabs"]))
        .iter()
        .filter_map(|tab| at(tab, &["tabRenderer", "content", "sectionListRenderer"]))
        .collect()
}

fn parse_container(container: &Value, results: &mut Accumulator) {
    if let Some(card) = container.get("musicCardShelfRenderer") {
        if let Some(item) = parse_renderer(card, &card_endpoints(card)) {
            results.push(item);
        }
        for content in array(card.get("contents")) {
            parse_container(content, results);
        }
        results.capture(continuation_token(card));
        return;
    }
    if let Some(shelf) = container.get("musicShelfRenderer") {
        results.capture(continuation_token(shelf));
        for content in array(shelf.get("contents")) {
            parse_container(content, results);
        }
        return;
    }
    if let Some(section) = container.get("itemSectionRenderer") {
        for content in array(section.get("contents")) {
            parse_container(content, results);
        }
        return;
    }
    let item = if let Some(renderer) = container.get("musicResponsiveListItemRenderer") {
        parse_renderer(renderer, &responsive_endpoints(renderer))
    } else if let Some(renderer) = container.get("musicTwoRowItemRenderer") {
        parse_renderer(renderer, &two_row_endpoints(renderer))
    } else if let Some(renderer) = container.get("musicMultiRowListItemRenderer") {
        parse_renderer(renderer, &multi_row_endpoints(renderer))
    } else {
        results.capture(continuation_item_token(container));
        return;
    };
    match item {
        Some(item) => results.push(item),
        None => tracing::debug!("skipping unparsed search result"),
    }
}

fn continuation_token(renderer: &Value) -> Option<&str> {
    for continuation in array(renderer.get("continuations")) {
        for key in ["nextContinuationData", "reloadContinuationData"] {
            if let Some(token) =
                str_at(continuation, &[key, "continuation"]).filter(|token| !token.is_empty())
            {
                return Some(token);
            }
        }
    }
    array(renderer.get("contents"))
        .iter()
        .find_map(continuation_item_token)
}

fn continuation_item_token(item: &Value) -> Option<&str> {
    str_at(
        item,
        &[
            "continuationItemRenderer",
            "continuationEndpoint",
            "continuationCommand",
            "token",
        ],
    )
    .filter(|token| !token.is_empty())
}

// MARK: - Endpoints

/// An endpoint, unwrapped from `innertubeCommand` when present.
fn unwrap_endpoint(value: Option<&Value>) -> Option<&Value> {
    let endpoint = value.filter(|value| value.is_object())?;
    Some(endpoint.get("innertubeCommand").unwrap_or(endpoint))
}

fn play_endpoint(overlay: Option<&Value>) -> Option<&Value> {
    unwrap_endpoint(at(
        overlay?,
        &[
            "musicItemThumbnailOverlayRenderer",
            "content",
            "musicPlayButtonRenderer",
            "playNavigationEndpoint",
        ],
    ))
}

fn title_run(renderer: &Value) -> Option<&Value> {
    let flex_title = at(
        renderer,
        &[
            "flexColumns",
            "0",
            "musicResponsiveListItemFlexColumnRenderer",
            "text",
            "runs",
            "0",
        ],
    );
    flex_title.or_else(|| at(renderer, &["title", "runs", "0"]))
}

fn collect(candidates: [Option<&Value>; 5]) -> Vec<&Value> {
    candidates
        .into_iter()
        .filter_map(|candidate| unwrap_endpoint(candidate))
        .collect()
}

fn responsive_endpoints(renderer: &Value) -> Vec<&Value> {
    collect([
        renderer.get("navigationEndpoint"),
        title_run(renderer).and_then(|run| run.get("navigationEndpoint")),
        renderer.get("onTap"),
        play_endpoint(renderer.get("overlay")),
        None,
    ])
}

fn two_row_endpoints(renderer: &Value) -> Vec<&Value> {
    collect([
        renderer.get("navigationEndpoint"),
        title_run(renderer).and_then(|run| run.get("navigationEndpoint")),
        renderer.get("onTap"),
        play_endpoint(renderer.get("thumbnailOverlay")),
        play_endpoint(renderer.get("overlay")),
    ])
}

fn multi_row_endpoints(renderer: &Value) -> Vec<&Value> {
    collect([
        renderer.get("onTap"),
        renderer.get("navigationEndpoint"),
        title_run(renderer).and_then(|run| run.get("navigationEndpoint")),
        play_endpoint(renderer.get("overlay")),
        None,
    ])
}

fn card_endpoints(card: &Value) -> Vec<&Value> {
    collect([
        title_run(card).and_then(|run| run.get("navigationEndpoint")),
        card.get("navigationEndpoint"),
        card.get("onTap"),
        play_endpoint(card.get("thumbnailOverlay")),
        play_endpoint(card.get("overlay")),
    ])
}

// MARK: - Items

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentKind {
    Song,
    Video,
    Album,
    Audiobook,
    Artist,
    Profile,
    Playlist,
    PodcastShow,
    PodcastEpisode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowseKind {
    Album,
    Artist,
    Profile,
    Playlist,
    PodcastShow,
    PodcastEpisode,
}

fn parse_renderer(renderer: &Value, endpoints: &[&Value]) -> Option<SearchItem> {
    let title = item_title(renderer).unwrap_or_else(|| "Unknown".into());
    let thumbnail = thumbnail_url(renderer);
    let kind = content_kind(renderer);

    let mut episode_browse = false;
    for endpoint in endpoints {
        let Some(browse) = endpoint.get("browseEndpoint") else {
            continue;
        };
        let Some(browse_id) = str_at(browse, &["browseId"]) else {
            continue;
        };
        let Some(browse_kind) = browse_kind(browse_id, page_type(browse), kind) else {
            continue;
        };
        if browse_kind == BrowseKind::PodcastEpisode {
            episode_browse = true;
            continue;
        }
        return Some(browse_item(
            browse_kind,
            browse_id,
            title,
            thumbnail,
            renderer,
        ));
    }

    for endpoint in endpoints {
        let watch_playlist = str_at(endpoint, &["watchPlaylistEndpoint", "playlistId"]);
        let contextual = (kind == Some(ContentKind::Playlist))
            .then(|| str_at(endpoint, &["watchEndpoint", "playlistId"]))
            .flatten();
        if let Some(id) = watch_playlist
            .or(contextual)
            .filter(|id| is_playlist_id(id))
        {
            return Some(browse_item(
                BrowseKind::Playlist,
                id,
                title,
                thumbnail,
                renderer,
            ));
        }
    }

    let (video_id, video_type) = playable(renderer, endpoints)?;
    let song = Song {
        video_id: video_id.to_owned(),
        title,
        artists: Vec::new(),
        thumbnail_url: thumbnail,
    };
    if episode_browse
        || kind == Some(ContentKind::PodcastEpisode)
        || video_type == Some("MUSIC_VIDEO_TYPE_PODCAST_EPISODE")
    {
        return Some(SearchItem::Episode(Song {
            artists: episode_show(renderer).into_iter().collect(),
            ..song
        }));
    }
    let song = Song {
        artists: item_artists(renderer, true),
        ..song
    };
    let is_video = kind == Some(ContentKind::Video)
        || matches!(
            video_type,
            Some(
                "MUSIC_VIDEO_TYPE_OMV"
                    | "MUSIC_VIDEO_TYPE_UGC"
                    | "MUSIC_VIDEO_TYPE_OFFICIAL_SOURCE_MUSIC"
            )
        );
    Some(if is_video {
        SearchItem::Video(song)
    } else {
        SearchItem::Song(song)
    })
}

fn browse_item(
    kind: BrowseKind,
    browse_id: &str,
    title: String,
    thumbnail_url: Option<String>,
    renderer: &Value,
) -> SearchItem {
    let card = BrowseCard {
        browse_id: browse_id.to_owned(),
        title,
        subtitle: semantic_subtitle(renderer),
        thumbnail_url,
    };
    match kind {
        BrowseKind::Album => SearchItem::Album(card),
        BrowseKind::Artist => SearchItem::Artist(card),
        BrowseKind::Profile => SearchItem::Profile(card),
        BrowseKind::Playlist => SearchItem::Playlist(card),
        // Episodes never reach here: they become playable items.
        BrowseKind::PodcastShow | BrowseKind::PodcastEpisode => SearchItem::Podcast(card),
    }
}

fn is_playlist_id(id: &str) -> bool {
    ["VL", "PL", "VM", "RD"]
        .iter()
        .any(|prefix| id.starts_with(prefix))
}

/// pageType first, then browseId prefix, as in the Swift parser.
fn browse_kind(
    browse_id: &str,
    page_type: Option<&str>,
    content: Option<ContentKind>,
) -> Option<BrowseKind> {
    match page_type {
        Some("MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK") => {
            return Some(BrowseKind::Album);
        }
        Some("MUSIC_PAGE_TYPE_USER_CHANNEL") => return Some(BrowseKind::Profile),
        page_type if is_artist_page_type(page_type) => return Some(BrowseKind::Artist),
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => return Some(BrowseKind::Playlist),
        Some("MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE") => return Some(BrowseKind::PodcastShow),
        _ => {}
    }
    if browse_id.starts_with("MPSPP") {
        Some(BrowseKind::PodcastShow)
    } else if browse_id.starts_with("MPED") || content == Some(ContentKind::PodcastEpisode) {
        Some(BrowseKind::PodcastEpisode)
    } else if browse_id.starts_with("MPRE") || browse_id.starts_with("OLAK") {
        Some(BrowseKind::Album)
    } else if is_playlist_id(browse_id) {
        Some(BrowseKind::Playlist)
    } else if is_navigable_artist_id(browse_id) {
        Some(if content == Some(ContentKind::Profile) {
            BrowseKind::Profile
        } else {
            BrowseKind::Artist
        })
    } else {
        None
    }
}

/// The video id and music video type: `playlistItemData` first, then the
/// first endpoint with a `watchEndpoint`.
fn playable<'a>(
    renderer: &'a Value,
    endpoints: &[&'a Value],
) -> Option<(&'a str, Option<&'a str>)> {
    let renderer_type = video_type(renderer.get("navigationEndpoint"))
        .or_else(|| video_type(play_endpoint(renderer.get("overlay"))));
    let first_type = renderer_type.or_else(|| {
        endpoints
            .iter()
            .find_map(|endpoint| video_type(Some(endpoint)))
    });
    if let Some(id) = str_at(renderer, &["playlistItemData", "videoId"]).filter(|id| !id.is_empty())
    {
        return Some((id, first_type));
    }
    endpoints.iter().find_map(|endpoint| {
        let id = str_at(endpoint, &["watchEndpoint", "videoId"]).filter(|id| !id.is_empty())?;
        Some((id, video_type(Some(endpoint)).or(first_type)))
    })
}

fn video_type(endpoint: Option<&Value>) -> Option<&str> {
    str_at(
        endpoint?,
        &[
            "watchEndpoint",
            "watchEndpointMusicSupportedConfigs",
            "watchEndpointMusicConfig",
            "musicVideoType",
        ],
    )
}

fn item_title(renderer: &Value) -> Option<String> {
    let text = title_run(renderer)?.get("text")?.as_str()?;
    Some(text.to_owned())
}

// MARK: - Metadata

/// Runs of the flex columns after the title, or else `subtitle` and
/// `secondSubtitle`.
fn metadata_runs(renderer: &Value) -> Vec<&Value> {
    let flex: Vec<&Value> = array(renderer.get("flexColumns"))
        .iter()
        .skip(1)
        .flat_map(|column| {
            array(at(
                column,
                &["musicResponsiveListItemFlexColumnRenderer", "text", "runs"],
            ))
        })
        .collect();
    if !flex.is_empty() {
        return flex;
    }
    ["subtitle", "secondSubtitle"]
        .iter()
        .flat_map(|key| array(at(renderer, &[key, "runs"])))
        .collect()
}

fn split_metadata(text: &str) -> impl Iterator<Item = &str> {
    text.split(['•', '·'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

fn content_kind(renderer: &Value) -> Option<ContentKind> {
    metadata_runs(renderer)
        .into_iter()
        .filter_map(|run| run.get("text")?.as_str())
        .flat_map(split_metadata)
        .find_map(content_kind_label)
}

/// Metadata parts that are not content type labels.
fn metadata_components(renderer: &Value) -> Vec<String> {
    metadata_runs(renderer)
        .into_iter()
        .filter_map(|run| run.get("text")?.as_str())
        .flat_map(split_metadata)
        .filter(|part| content_kind_label(part).is_none())
        .map(str::to_owned)
        .collect()
}

fn semantic_subtitle(renderer: &Value) -> Option<String> {
    let components = metadata_components(renderer);
    (!components.is_empty()).then(|| components.join(" • "))
}

/// Flex-column artists first, then linked artist runs, then (for songs and
/// videos) the first plain metadata part that is not a count, date, or
/// duration.
fn item_artists(renderer: &Value, plain_text_fallback: bool) -> Vec<ArtistRef> {
    if renderer.get("flexColumns").is_some() {
        let column = at(
            renderer,
            &[
                "flexColumns",
                "1",
                "musicResponsiveListItemFlexColumnRenderer",
                "text",
            ],
        );
        let artists: Vec<ArtistRef> = artists_from_flex_column(column)
            .into_iter()
            .filter(|artist| {
                artist.browse_id.is_some() || content_kind_label(&artist.name).is_none()
            })
            .collect();
        if !artists.is_empty() {
            return artists;
        }
    }
    let linked: Vec<ArtistRef> = metadata_runs(renderer)
        .into_iter()
        .filter_map(|run| {
            let name = run.get("text")?.as_str()?.trim();
            let browse = unwrap_endpoint(run.get("navigationEndpoint"))?.get("browseEndpoint")?;
            let id = str_at(browse, &["browseId"])?;
            (!name.is_empty()
                && (is_navigable_artist_id(id) || is_artist_page_type(page_type(browse))))
            .then(|| ArtistRef {
                name: name.to_owned(),
                browse_id: Some(id.to_owned()),
            })
        })
        .collect();
    if !linked.is_empty() || !plain_text_fallback {
        return linked;
    }
    metadata_components(renderer)
        .into_iter()
        .find(|part| !is_non_artist_metadata(part))
        .map(|name| ArtistRef {
            name,
            browse_id: None,
        })
        .into_iter()
        .collect()
}

/// The show an episode links to (`MPSPP…` browse id).
fn episode_show(renderer: &Value) -> Option<ArtistRef> {
    metadata_runs(renderer).into_iter().find_map(|run| {
        let browse = unwrap_endpoint(run.get("navigationEndpoint"))?.get("browseEndpoint")?;
        let id = str_at(browse, &["browseId"]).filter(|id| id.starts_with("MPSPP"))?;
        let name = run.get("text")?.as_str()?.trim();
        (!name.is_empty()).then(|| ArtistRef {
            name: name.to_owned(),
            browse_id: Some(id.to_owned()),
        })
    })
}

static NON_ARTIST_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)^\s*\d+(?:[.,]\d+)?\s*(?:seconds?|secs?|minutes?|mins?|hours?|hrs?)\s*$",
        r"(?i)^\s*\d+\s*(?:seconds?|secs?|minutes?|mins?|hours?|hrs?|days?|weeks?|months?|years?|[smhdwy])\s+ago\s*$",
        r"(?i)^\s*(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+\d{1,2}(?:,\s*\d{4})?\s*$",
        r"^\s*(?:\d{4}|\d{4}-\d{1,2}-\d{1,2})\s*$",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).expect("static regex is valid"))
    .collect()
});

fn is_non_artist_metadata(text: &str) -> bool {
    content_kind_label(text).is_some()
        || is_clock_duration(text.trim())
        || has_count_unit(text)
        || NON_ARTIST_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(text))
}

/// "12 songs", "42K subscribers", "1.2M views", and localized variants.
fn has_count_unit(text: &str) -> bool {
    let normalized = text
        .replace(['\u{00A0}', '\u{202F}'], " ")
        .trim()
        .to_lowercase();
    if !normalized.chars().any(char::is_numeric) {
        return false;
    }
    let Some(unit) = normalized
        .split(|ch: char| ch.is_whitespace() || ch.is_numeric() || ch == '.' || ch == ',')
        .rfind(|part| !part.is_empty())
    else {
        return false;
    };
    COUNT_UNITS.contains(&unit)
        || ["выпуск", "odcin", "епізод"]
            .iter()
            .any(|prefix| unit.starts_with(prefix))
}

// Kept in sync with SearchResponseParser+Support.swift `countMetadataUnits`.
#[rustfmt::skip]
const COUNT_UNITS: &[&str] = &[
    "subscriber", "subscribers", "view", "views", "play", "plays", "episode", "episodes", "song", "songs",
    "track", "tracks", "حلقة", "الحلقات", "أغنية", "أغاني", "أغانٍ", "مقطوعات", "folge", "folgen",
    "titel", "titeln", "episodio", "episodios", "canción", "canciones", "pista", "pistas", "épisode",
    "épisodes", "morceau", "morceaux", "titre", "titres", "lagu", "trek", "episodi", "brano", "brani",
    "에피소드", "노래", "곡", "트랙", "aflevering", "afleveringen", "nummer", "nummers", "odcinek",
    "odcinki", "utwór", "utwory", "utworów", "utworami", "episódio", "episódios", "música", "músicas",
    "faixa", "faixas", "выпуск", "выпуски", "трек", "треки", "треков", "avsnitt", "låt", "låtar",
    "spår", "bölüm", "bölümler", "şarkı", "şarkılar", "parça", "епізод", "епізоди", "пісня",
    "пісні", "пісень", "треків",
];

// Kept in sync with SearchResponseParser.swift `contentKind(fromLabel:)`.
fn content_kind_label(label: &str) -> Option<ContentKind> {
    let label = label.trim().to_lowercase();
    let kind = match label.as_str() {
        "song" | "أغنية" | "titel" | "canción" | "morceau" | "lagu" | "brano" | "노래"
        | "nummer" | "utwór" | "música" | "трек" | "låt" | "şarkı" | "пісня" => {
            ContentKind::Song
        }
        "video" | "فيديو" | "vídeo" | "vidéo" | "동영상" | "wideo" | "видео" | "відео" => {
            ContentKind::Video
        }
        "album" | "single" | "ep" | "ألبوم" | "álbum" | "앨범" | "альбом" | "albüm" => {
            ContentKind::Album
        }
        "audiobook"
        | "كتاب صوتي"
        | "hörbuch"
        | "audiolibro"
        | "livre audio"
        | "buku audio"
        | "오디오북"
        | "luisterboek"
        | "аудиокнига"
        | "ljudbok"
        | "sesli kitap"
        | "аудіокнига" => ContentKind::Audiobook,
        "artist"
        | "فنان"
        | "interpret"
        | "artista"
        | "artiste"
        | "artis"
        | "아티스트"
        | "artiest"
        | "artysta"
        | "исполнитель"
        | "sanatçı"
        | "виконавець" => ContentKind::Artist,
        "profile"
        | "الملف الشخصي"
        | "profil"
        | "perfil"
        | "profilo"
        | "프로필"
        | "profiel"
        | "профиль"
        | "профіль" => ContentKind::Profile,
        "playlist"
        | "قائمة تشغيل"
        | "lista de reproducción"
        | "daftar putar"
        | "재생목록"
        | "afspeellijst"
        | "playlista"
        | "lista de reprodução"
        | "плейлист"
        | "spellista"
        | "çalma listesi" => ContentKind::Playlist,
        "podcast" | "بودكاست" | "팟캐스트" | "подкаст" | "podd" => {
            ContentKind::PodcastShow
        }
        "episode" | "podcast episode" | "حلقة" | "folge" | "episodio" | "épisode" | "에피소드"
        | "aflevering" | "odcinek" | "episódio" | "выпуск" | "avsnitt" | "bölüm" | "епізод" => {
            ContentKind::PodcastEpisode
        }
        _ => return None,
    };
    Some(kind)
}
```

- [ ] **Step 7: Run the checks**

Run: `cargo fmt --all --check && cargo clippy -p ytm-core --all-targets -- -D warnings && cargo test -p ytm-core --features specta`
Expected: `search_parser` reports `11 passed`; the lib tests report `17 passed` with `--features specta`; `home_parser` 11, `client` 8, `api_key` 4.

- [ ] **Step 8: Commit and push**

```bash
git add crates/ytm-core/src/models.rs crates/ytm-core/src/parsers/helpers.rs crates/ytm-core/src/parsers/mod.rs crates/ytm-core/src/parsers/search.rs crates/ytm-core/tests/search_parser.rs crates/ytm-core/tests/snapshots/search_parser__parses_shelves_in_server_order.snap crates/ytm-core/tests/fixtures/search_*.json
git commit -m "feat(core): port the search response parser"
git push origin main
```

---

### Task 3: Search suggestions, client calls, and `ytm-explorer search` / `suggest`

**Files:**
- Modify: `crates/ytm-core/src/parsers/mod.rs`, `crates/ytm-core/src/client.rs`, `crates/ytm-explorer/src/main.rs`
- Create: `crates/ytm-core/src/parsers/search_suggestions.rs`, `crates/ytm-core/tests/search_suggestions_parser.rs`
- Test: `crates/ytm-core/tests/client.rs` (append)

**Interfaces:**
- Consumes: `InnerTubeClient::post(endpoint, body)` (adds `context`, auth headers, maps 401/403 → `AuthExpired`); `parse_search` (Task 2); `parse_home`.
- Produces: `InnerTubeClient::search(&self, query: &str) -> Result<Value, YtmError>` (POST `search` with `{ "query": … }`), `InnerTubeClient::search_suggestions(&self, input: &str) -> Result<Value, YtmError>` (POST `music/get_search_suggestions` with `{ "input": … }`), `ytm_core::parsers::search_suggestions::parse_search_suggestions(&Value) -> Vec<String>` (text and history suggestions in order, deduplicated, rich suggestions skipped). `ytm-explorer search <QUERY> [--parse] [--out FILE]` and `ytm-explorer suggest <INPUT> [--parse] [--out FILE]`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ytm-core/tests/search_suggestions_parser.rs`:

```rust
use serde_json::{Value, json};
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

fn suggestion(kind: &str, runs: Value) -> Value {
    json!({ kind: { "suggestion": { "runs": runs } } })
}

#[test]
fn suggestions_keep_order_and_drop_duplicates() {
    let response = json!({ "contents": [
        { "searchSuggestionsSectionRenderer": { "contents": [
            suggestion("historySuggestionRenderer", json!([{ "text": "taylor swift" }])),
            suggestion("searchSuggestionRenderer", json!([{ "text": "taylor" }, { "text": " swift songs" }])),
            suggestion("searchSuggestionRenderer", json!([{ "text": "taylor swift" }])),
            suggestion("searchSuggestionRenderer", json!([]))
        ] } },
        { "searchSuggestionsSectionRenderer": { "contents": [
            { "musicResponsiveListItemRenderer": {} },
            suggestion("searchSuggestionRenderer", json!([{ "text": "taylor swift playlist" }]))
        ] } }
    ] });
    assert_eq!(
        parse_search_suggestions(&response),
        [
            "taylor swift",
            "taylor swift songs",
            "taylor swift playlist"
        ]
    );
}

#[test]
fn suggestions_without_contents_are_empty() {
    assert!(parse_search_suggestions(&json!({})).is_empty());
    assert!(parse_search_suggestions(&json!({ "contents": [] })).is_empty());
}
```

Append to the end of `crates/ytm-core/tests/client.rs`:

```rust
#[tokio::test]
async fn search_posts_the_query() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(query_param("key", "test-api-key"))
        .and(body_partial_json(json!({
            "query": "test query",
            "context": { "client": { "clientName": "WEB_REMIX" } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "contents": {} })))
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert_eq!(
        client.search("test query").await.unwrap(),
        json!({ "contents": {} })
    );
}

#[tokio::test]
async fn search_suggestions_post_the_input() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/music/get_search_suggestions"))
        .and(body_partial_json(json!({ "input": "test inp" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "contents": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert_eq!(
        client.search_suggestions("test inp").await.unwrap(),
        json!({ "contents": [] })
    );
}

#[tokio::test]
async fn search_maps_auth_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert!(matches!(
        client.search("test query").await,
        Err(YtmError::AuthExpired)
    ));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ytm-core --test search_suggestions_parser --test client`
Expected: FAIL to compile — no `parsers::search_suggestions`, no method `search` / `search_suggestions` on `InnerTubeClient`.

- [ ] **Step 3: Implement the suggestions parser**

Replace `crates/ytm-core/src/parsers/mod.rs` with:

```rust
mod helpers;
pub mod home;
pub mod search;
pub mod search_suggestions;
```

Create `crates/ytm-core/src/parsers/search_suggestions.rs`:

```rust
//! `music/get_search_suggestions` responses, ported from the Swift
//! `SearchSuggestionsParser`.

use std::collections::HashSet;

use serde_json::Value;

/// Text and history suggestions in order, without duplicates. Rich
/// suggestions (songs, artists) are skipped, as in the Swift app.
pub fn parse_search_suggestions(response: &Value) -> Vec<String> {
    let mut seen = HashSet::new();
    let sections = response
        .get("contents")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    sections
        .iter()
        .filter_map(|section| {
            section
                .get("searchSuggestionsSectionRenderer")?
                .get("contents")?
                .as_array()
        })
        .flatten()
        .filter_map(|item| {
            let renderer = item
                .get("searchSuggestionRenderer")
                .or_else(|| item.get("historySuggestionRenderer"))?;
            let text: String = renderer
                .get("suggestion")?
                .get("runs")?
                .as_array()?
                .iter()
                .filter_map(|run| run.get("text")?.as_str())
                .collect();
            (!text.is_empty()).then_some(text)
        })
        .filter(|text| seen.insert(text.clone()))
        .collect()
}
```

- [ ] **Step 4: Add the client calls**

In `crates/ytm-core/src/client.rs`, replace the end of `impl InnerTubeClient` — the lines

```rust
        self.post("browse", body).await
    }
}
```

— with:

```rust
        self.post("browse", body).await
    }

    /// POST `search` without filter params: the mixed first page (top result,
    /// songs, videos, albums, artists, playlists, …).
    pub async fn search(&self, query: &str) -> Result<Value, YtmError> {
        let mut body = Map::new();
        body.insert("query".into(), query.into());
        self.post("search", body).await
    }

    /// POST `music/get_search_suggestions` for the text typed so far.
    pub async fn search_suggestions(&self, input: &str) -> Result<Value, YtmError> {
        let mut body = Map::new();
        body.insert("input".into(), input.into());
        self.post("music/get_search_suggestions", body).await
    }
}
```

- [ ] **Step 5: Add the explorer subcommands**

Replace `crates/ytm-explorer/src/main.rs` with:

```rust
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::auth::CookieJar;
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};
use ytm_core::parsers::home::parse_home;
use ytm_core::parsers::search::parse_search;
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

/// Probe YouTube Music InnerTube endpoints (read-only).
///
/// KASET_COOKIE_FILE: path to a file containing a Cookie header. Keep it
/// outside the repository. Omit it for signed-out requests.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// POST /browse and print the raw JSON, or the parsed model with --parse.
    Browse {
        browse_id: String,
        #[arg(long)]
        parse: Option<ParseAs>,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /search and print the raw JSON, or the parsed results with --parse.
    Search {
        query: String,
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /music/get_search_suggestions and print the raw JSON, or the
    /// parsed suggestions with --parse.
    Suggest {
        input: String,
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
enum ParseAs {
    Home,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();

    let api_key = match std::env::var(API_KEY_ENV) {
        Ok(key) if !key.trim().is_empty() => key.trim().to_owned(),
        _ => fetch_api_key(MUSIC_ORIGIN)
            .await
            .context("resolving API key")?,
    };
    let client = InnerTubeClient::new(ClientConfig::music(api_key), load_cookies()?)?;

    let (text, out) = match cli.command {
        Command::Browse {
            browse_id,
            parse,
            out,
        } => {
            let raw = client.browse(&browse_id).await?;
            let text = match parse {
                Some(ParseAs::Home) => serde_json::to_string_pretty(&parse_home(&raw)?)?,
                None => serde_json::to_string_pretty(&raw)?,
            };
            (text, out)
        }
        Command::Search { query, parse, out } => {
            let raw = client.search(&query).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_search(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
        Command::Suggest { input, parse, out } => {
            let raw = client.search_suggestions(&input).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_search_suggestions(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
    };
    match out {
        Some(path) => {
            std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?
        }
        None => println!("{text}"),
    }
    Ok(())
}

fn load_cookies() -> Result<Option<CookieJar>> {
    let Some(path) = std::env::var_os("KASET_COOKIE_FILE") else {
        return Ok(None);
    };
    let header = std::fs::read_to_string(&path).context("reading KASET_COOKIE_FILE")?;
    Ok(Some(CookieJar::from_header(&header)))
}
```

- [ ] **Step 6: Run the checks**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ytm-core -p ytm-explorer && cargo run -q -p ytm-explorer -- search --help`
Expected: `client` 11 passed, `search_suggestions_parser` 2 passed, `search_parser` 11, `home_parser` 11; the help text lists `--parse` and `--out`. Do not run the subcommands without `--help`: that sends a live request.

- [ ] **Step 7: Commit and push**

```bash
git add crates/ytm-core/src/parsers/mod.rs crates/ytm-core/src/parsers/search_suggestions.rs crates/ytm-core/src/client.rs crates/ytm-core/tests/search_suggestions_parser.rs crates/ytm-core/tests/client.rs crates/ytm-explorer/src/main.rs
git commit -m "feat(core): add search and suggestion requests and explorer subcommands"
git push origin main
```

The live check of these requests is Task 4 Step 8.

---

### Task 4: Account summary — parser, client call, `ytm-explorer account`

The sidebar's account block needs the signed-in account's name. The Swift `AccountsListParser` reads `account/accounts_list`; this task keeps only the selected account's name, handle, and photo (the email and brand-account tokens are not read). It also hosts the human verification step for every request added in Tasks 2–4.

**Files:**
- Modify: `crates/ytm-core/src/models.rs`, `crates/ytm-core/src/parsers/mod.rs`, `crates/ytm-core/src/client.rs`, `crates/ytm-explorer/src/main.rs`
- Create: `crates/ytm-core/src/parsers/account.rs`, `crates/ytm-core/tests/account_parser.rs`
- Test: `crates/ytm-core/tests/client.rs` (append)

**Interfaces:**
- Consumes: `InnerTubeClient::post`, `json::{at, str_at}`.
- Produces: `ytm_core::models::Account { name: String, handle: Option<String>, photo_url: Option<String> }` (`Serialize`, `specta::Type` behind the feature); `ytm_core::parsers::account::parse_account(&Value) -> Option<Account>` (the `isSelected` account, else the first named one; protocol-relative photo URLs become https); `InnerTubeClient::accounts_list(&self) -> Result<Value, YtmError>` (POST `account/accounts_list` with an empty body plus `context`); `ytm-explorer account [--parse] [--out FILE]`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ytm-core/tests/account_parser.rs`:

```rust
use serde_json::{Value, json};
use ytm_core::models::Account;
use ytm_core::parsers::account::parse_account;

fn item(name: &str, handle: Option<&str>, selected: bool) -> Value {
    let mut item = json!({
        "accountName": { "runs": [{ "text": name }] },
        "accountPhoto": { "thumbnails": [
            { "url": "//yt3.ggpht.com/test-small=s48" },
            { "url": "//yt3.ggpht.com/test-large=s88" }
        ] },
        "isSelected": selected
    });
    if let Some(handle) = handle {
        item["channelHandle"] = json!({ "runs": [{ "text": handle }] });
    }
    json!({ "accountItem": item })
}

fn response(items: Vec<Value>) -> Value {
    json!({ "actions": [{ "getMultiPageMenuAction": { "menu": { "multiPageMenuRenderer": {
        "sections": [
            { "accountSectionListRenderer": {
                "header": { "googleAccountHeaderRenderer": {
                    "email": { "runs": [{ "text": "test@example.com" }] } } },
                "contents": [{ "accountItemSectionRenderer": { "contents": items } }]
            } }
        ]
    } } } }] })
}

#[test]
fn picks_the_selected_account() {
    let account = parse_account(&response(vec![
        item("Brand Account", Some("@brandaccount"), false),
        item("Test User", Some("@testuser"), true),
    ]));
    assert_eq!(
        account,
        Some(Account {
            name: "Test User".into(),
            handle: Some("@testuser".into()),
            photo_url: Some("https://yt3.ggpht.com/test-large=s88".into()),
        })
    );
}

#[test]
fn falls_back_to_the_first_account() {
    let account = parse_account(&response(vec![
        item("Test User", None, false),
        item("Brand Account", Some("@brandaccount"), false),
    ]))
    .unwrap();
    assert_eq!(account.name, "Test User");
    assert_eq!(account.handle, None);
}

#[test]
fn skips_nameless_items_and_empty_responses() {
    assert_eq!(parse_account(&json!({})), None);
    assert_eq!(parse_account(&json!({ "actions": [] })), None);
    assert_eq!(parse_account(&response(vec![item("", None, true)])), None);
}
```

Append to the end of `crates/ytm-core/tests/client.rs`:

```rust
#[tokio::test]
async fn accounts_list_posts_an_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/account/accounts_list"))
        .and(body_partial_json(json!({
            "context": { "client": { "clientName": "WEB_REMIX" } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "actions": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let jar = CookieJar::from_header("SAPISID=test-sapisid");
    let client = InnerTubeClient::new(config_for(&server), Some(jar)).unwrap();
    assert_eq!(
        client.accounts_list().await.unwrap(),
        json!({ "actions": [] })
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ytm-core --test account_parser --test client`
Expected: FAIL to compile — no `models::Account`, no `parsers::account`, no method `accounts_list`.

- [ ] **Step 3: Add the model**

In `crates/ytm-core/src/models.rs`, insert immediately above the line `#[cfg(all(test, feature = "specta"))]`:

```rust
/// The signed-in YouTube Music account, for display only.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Account {
    pub name: String,
    /// Channel handle such as `@name`, when the account has one.
    pub handle: Option<String>,
    pub photo_url: Option<String>,
}

```

In the same file's `models_implement_specta_type` test, after `assert_type::<SearchResults>();` add:

```rust
        assert_type::<Account>();
```

- [ ] **Step 4: Implement the parser and the client call**

Replace `crates/ytm-core/src/parsers/mod.rs` with:

```rust
pub mod account;
mod helpers;
pub mod home;
pub mod search;
pub mod search_suggestions;
```

Create `crates/ytm-core/src/parsers/account.rs`:

```rust
//! `account/accounts_list` responses, reduced from the Swift
//! `AccountsListParser` to what the sidebar shows. The Google email and
//! brand-account tokens are deliberately not read.

use serde_json::Value;

use crate::json::{at, str_at};
use crate::models::Account;

const SECTIONS: &[&str] = &[
    "actions",
    "0",
    "getMultiPageMenuAction",
    "menu",
    "multiPageMenuRenderer",
    "sections",
];

/// The selected account, or the first listed one; `None` when the response
/// lists no account with a name.
pub fn parse_account(response: &Value) -> Option<Account> {
    let accounts: Vec<(bool, Account)> = at(response, SECTIONS)?
        .as_array()?
        .iter()
        .filter_map(|section| {
            section
                .get("accountSectionListRenderer")?
                .get("contents")?
                .as_array()
        })
        .flatten()
        .filter_map(|content| {
            content
                .get("accountItemSectionRenderer")?
                .get("contents")?
                .as_array()
        })
        .flatten()
        .filter_map(|wrapper| parse_item(wrapper.get("accountItem")?))
        .collect();
    let selected = accounts.iter().position(|(selected, _)| *selected);
    accounts
        .into_iter()
        .nth(selected.unwrap_or(0))
        .map(|(_, account)| account)
}

fn parse_item(item: &Value) -> Option<(bool, Account)> {
    let name =
        str_at(item, &["accountName", "runs", "0", "text"]).filter(|name| !name.is_empty())?;
    let handle = str_at(item, &["channelHandle", "runs", "0", "text"]).map(str::to_owned);
    let photo_url = item
        .get("accountPhoto")
        .and_then(|photo| photo.get("thumbnails"))
        .and_then(Value::as_array)
        .and_then(|thumbnails| thumbnails.last())
        .and_then(|thumbnail| thumbnail.get("url"))
        .and_then(Value::as_str)
        .map(|url| match url.strip_prefix("//") {
            Some(rest) => format!("https://{rest}"),
            None => url.to_owned(),
        });
    let selected = item
        .get("isSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some((
        selected,
        Account {
            name: name.to_owned(),
            handle,
            photo_url,
        },
    ))
}
```

In `crates/ytm-core/src/client.rs`, replace

```rust
        self.post("music/get_search_suggestions", body).await
    }
}
```

with

```rust
        self.post("music/get_search_suggestions", body).await
    }

    /// POST `account/accounts_list`: the Google account's YouTube identities.
    pub async fn accounts_list(&self) -> Result<Value, YtmError> {
        self.post("account/accounts_list", Map::new()).await
    }
}
```

- [ ] **Step 5: Add the explorer subcommand**

Replace `crates/ytm-explorer/src/main.rs` with:

```rust
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::auth::CookieJar;
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};
use ytm_core::parsers::account::parse_account;
use ytm_core::parsers::home::parse_home;
use ytm_core::parsers::search::parse_search;
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

/// Probe YouTube Music InnerTube endpoints (read-only).
///
/// KASET_COOKIE_FILE: path to a file containing a Cookie header. Keep it
/// outside the repository. Omit it for signed-out requests.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// POST /browse and print the raw JSON, or the parsed model with --parse.
    Browse {
        browse_id: String,
        #[arg(long)]
        parse: Option<ParseAs>,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /search and print the raw JSON, or the parsed results with --parse.
    Search {
        query: String,
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /music/get_search_suggestions and print the raw JSON, or the
    /// parsed suggestions with --parse.
    Suggest {
        input: String,
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /account/accounts_list (needs KASET_COOKIE_FILE) and print the raw
    /// JSON, or the selected account with --parse.
    Account {
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
enum ParseAs {
    Home,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();

    let api_key = match std::env::var(API_KEY_ENV) {
        Ok(key) if !key.trim().is_empty() => key.trim().to_owned(),
        _ => fetch_api_key(MUSIC_ORIGIN)
            .await
            .context("resolving API key")?,
    };
    let client = InnerTubeClient::new(ClientConfig::music(api_key), load_cookies()?)?;

    let (text, out) = match cli.command {
        Command::Browse {
            browse_id,
            parse,
            out,
        } => {
            let raw = client.browse(&browse_id).await?;
            let text = match parse {
                Some(ParseAs::Home) => serde_json::to_string_pretty(&parse_home(&raw)?)?,
                None => serde_json::to_string_pretty(&raw)?,
            };
            (text, out)
        }
        Command::Search { query, parse, out } => {
            let raw = client.search(&query).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_search(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
        Command::Suggest { input, parse, out } => {
            let raw = client.search_suggestions(&input).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_search_suggestions(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
        Command::Account { parse, out } => {
            let raw = client.accounts_list().await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_account(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
    };
    match out {
        Some(path) => {
            std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?
        }
        None => println!("{text}"),
    }
    Ok(())
}

fn load_cookies() -> Result<Option<CookieJar>> {
    let Some(path) = std::env::var_os("KASET_COOKIE_FILE") else {
        return Ok(None);
    };
    let header = std::fs::read_to_string(&path).context("reading KASET_COOKIE_FILE")?;
    Ok(Some(CookieJar::from_header(&header)))
}
```

- [ ] **Step 6: Run the checks**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ytm-core --features specta && cargo run -q -p ytm-explorer -- account --help`
Expected: `account_parser` 3 passed, `client` 12 passed, lib 17 passed (with `specta`), `search_parser` 11, `search_suggestions_parser` 2, `home_parser` 11, `api_key` 4.

- [ ] **Step 7: Commit and push**

```bash
git add crates/ytm-core/src/models.rs crates/ytm-core/src/parsers/mod.rs crates/ytm-core/src/parsers/account.rs crates/ytm-core/src/client.rs crates/ytm-core/tests/account_parser.rs crates/ytm-core/tests/client.rs crates/ytm-explorer/src/main.rs
git commit -m "feat(core): read the signed-in account summary"
git push origin main
```

- [ ] **Step 8: Human verification against live responses (required; a Codex worker records "pending human" and continues)**

The search, suggestion, and account parsers were written from the Swift parsers and sanitized fixtures because signed-out search in the human's region returns only a region message. The human, with a signed-in Cookie header saved in a file **outside the repository**:

```bash
export KASET_COOKIE_FILE=~/kaset-cookie.txt        # never commit or paste its contents
cargo run -q -p ytm-explorer -- search "newjeans" --out /tmp/kaset-search.json
cargo run -q -p ytm-explorer -- search "newjeans" --parse
cargo run -q -p ytm-explorer -- search "lofi hip hop radio" --parse
cargo run -q -p ytm-explorer -- suggest "newj" --parse
cargo run -q -p ytm-explorer -- account --parse
cargo run -q -p ytm-explorer -- account --out /tmp/kaset-account.json
cargo run -q -p ytm-explorer -- browse FEmusic_home --parse home
```

Check: the parsed search lists a top result plus songs, videos, albums, artists, and playlists in the order music.youtube.com shows them, each song has a title, artists, and a thumbnail URL on an `*.ytimg.com` / `*.googleusercontent.com` / `*.ggpht.com` host; `suggest` returns plain strings; `account --parse` prints the expected name and handle; the parsed home has no fewer sections than before Task 1. Search `/tmp/kaset-account.json` for a subscription indicator (for example the words "Premium" or "membership" in a renderer key or label) and record only the key path, not the value — it decides whether a later task can add a Premium label (Decision 12). If something is missing, grep the raw JSON for the renderer key and open a follow-up; if a new fixture is captured, scrub every name, email, handle, photo URL, `clickTrackingParams`, and token before committing it. Delete the `/tmp/kaset-*.json` files afterwards. Record the outcome (renderer keys and counts only) in `docs/superpowers/spikes/2026-11-22-tauri-playback.md` under a new "M4 API" heading.

---

### Task 5: Commands, native window vibrancy, bindings, image CSP

**Files:**
- Modify: `app/src-tauri/Cargo.toml`, `Cargo.lock`, `app/src-tauri/src/commands.rs`, `app/src-tauri/src/lib.rs`, `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/tauri.conf.json`
- Create: `app/src-tauri/src/window_effects.rs`
- Generated (commit): `app/src/bindings.ts`, `app/src-tauri/permissions/autogenerated/{search,search_suggestions,get_account,window_vibrancy}.toml`
- Test: `app/src-tauri/src/commands.rs` (tests module)

**Interfaces:**
- Consumes: `InnerTubeClient::{browse, search, search_suggestions, accounts_list}` (Tasks 3–4), `parse_home`, `parse_search`, `parse_search_suggestions`, `parse_account`, `AppState::{launch, session, client()}`, `on_error(app, epoch, error)` (existing, unchanged), `ErrorKind::InvalidArgument`; `window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial::Sidebar, NSVisualEffectState::FollowsWindowActiveState}` (macOS) and `window_vibrancy::{apply_mica, Error::UnsupportedPlatformVersion}` (Windows), both taking `impl HasWindowHandle` (implemented by `&WebviewWindow`).
- Produces: Tauri commands `search(query: String) -> Result<SearchResults, AppError>` (empty or >200 chars → `invalid_argument`), `search_suggestions(input: String) -> Result<Vec<String>, AppError>` (empty → `[]` without a request; >200 chars → `invalid_argument`), `get_account() -> Result<Option<Account>, AppError>`, `window_vibrancy() -> Result<bool, AppError>`; `pub const MAX_QUERY_CHARS: usize = 200`; private `signed_in_request(app, state, request)` used by every API command; `window_effects::{WindowEffects { vibrancy: bool }, apply(&WebviewWindow) -> bool}`. TS (generated): `commands.search(query)`, `commands.searchSuggestions(input)`, `commands.getAccount(): Promise<Account | null>`, `commands.windowVibrancy(): Promise<boolean>`, types `SearchItem`, `SearchResults`, `Account`; `MockCommandHandlers.{search: (args: { query: string }) => …, searchSuggestions: (args: { input: string }) => …, getAccount: () => …, windowVibrancy: () => …}`.

- [ ] **Step 1: Add the dependency**

In `app/src-tauri/Cargo.toml`, under `[dependencies]`, after the `tracing-subscriber` line add:

```toml
window-vibrancy = "0.8.1"
```

Run: `cargo tree -p kaset -i window-vibrancy --depth 1`
Expected: `window-vibrancy v0.8.1` used by both `kaset` and `tauri` (the same locked version; `Cargo.lock` only gains `"window-vibrancy"` in kaset's dependency list).

- [ ] **Step 2: Write the failing tests and the commands**

The tests sit in the same file as the code, so this step writes both. Replace `app/src-tauri/src/commands.rs` with:

```rust
use std::future::Future;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use ytm_core::client::InnerTubeClient;
use ytm_core::models::{Account, HomePage, SearchResults};
use ytm_core::parsers::account::parse_account;
use ytm_core::parsers::home::parse_home;
use ytm_core::parsers::search::parse_search;
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

use crate::error::{AppError, ErrorKind};
use crate::login::{LOGIN_LABEL, close_login_window, emit_auth_changed, open_login_window};
use crate::session::AuthStatus;
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::webview_session::{blocking, clear_webview_cookies};
use crate::window_effects::WindowEffects;

/// The current sign-in state. The UI calls this once at start and then
/// follows `auth://changed`.
#[tauri::command]
#[specta::specta]
pub async fn auth_status(state: State<'_, AppState>) -> Result<AuthStatus, AppError> {
    Ok(state.session.status())
}

/// Opens the Google sign-in window. Completion arrives as `auth://changed`.
#[tauri::command]
#[specta::specta]
pub async fn sign_in(app: AppHandle) -> Result<(), AppError> {
    open_login_window(&app)
}

/// Stops playback, forgets the stored session, and deletes the WebView's
/// Google cookies.
#[tauri::command]
#[specta::specta]
pub async fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        close_login_window(&window);
    }
    let result = state
        .player
        .stop_for_sign_out(&app, async {
            let task_app = app.clone();
            let result = blocking(move || {
                if let Some(window) = task_app.get_webview_window("main") {
                    clear_webview_cookies(&window, "sign-out");
                }
                task_app.state::<AppState>().session.sign_out()
            })
            .await;
            emit_auth_changed(&app, state.session.status());
            result
        })
        .await;
    result?
        .map(|_| ())
        .map_err(|error| AppError::internal(error.to_string()))
}

/// Fetches and parses the signed-in home feed.
#[tauri::command]
#[specta::specta]
pub async fn get_home(app: AppHandle, state: State<'_, AppState>) -> Result<HomePage, AppError> {
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_home(&client.browse("FEmusic_home").await?)?)
    })
    .await
}

/// The signed-in account's name, handle, and photo for the sidebar; `None`
/// when the response lists no named account.
#[tauri::command]
#[specta::specta]
pub async fn get_account(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Account>, AppError> {
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_account(&client.accounts_list().await?))
    })
    .await
}

/// Longest accepted search query or suggestion input, in characters.
pub const MAX_QUERY_CHARS: usize = 200;

/// Searches YouTube Music (the unfiltered first page). Rejects an empty or
/// overlong query with `invalid_argument`.
#[tauri::command]
#[specta::specta]
pub async fn search(
    app: AppHandle,
    state: State<'_, AppState>,
    query: String,
) -> Result<SearchResults, AppError> {
    let query = checked_query(&query)?
        .ok_or_else(|| AppError::new(ErrorKind::InvalidArgument, "Enter something to search for."))?
        .to_owned();
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_search(&client.search(&query).await?))
    })
    .await
}

/// Autocomplete suggestions for `input`. An empty input returns none
/// without a request.
#[tauri::command]
#[specta::specta]
pub async fn search_suggestions(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
) -> Result<Vec<String>, AppError> {
    let Some(input) = checked_query(&input)?.map(str::to_owned) else {
        return Ok(Vec::new());
    };
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_search_suggestions(
            &client.search_suggestions(&input).await?,
        ))
    })
    .await
}

/// The trimmed query; `None` when empty, `invalid_argument` when longer than
/// `MAX_QUERY_CHARS`.
fn checked_query(query: &str) -> Result<Option<&str>, AppError> {
    let query = query.trim();
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            format!("Search text is limited to {MAX_QUERY_CHARS} characters."),
        ));
    }
    Ok((!query.is_empty()).then_some(query))
}

/// Runs an API request for the signed-in session. Waits for the launch cookie
/// refresh first, and expires the session on 401/403 only if it is still the
/// session the request started with.
async fn signed_in_request<T, F, Fut>(
    app: &AppHandle,
    state: &AppState,
    request: F,
) -> Result<T, AppError>
where
    F: FnOnce(Arc<InnerTubeClient>) -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    if !state.launch.wait(LAUNCH_REFRESH_TIMEOUT).await {
        tracing::warn!("launch cookie refresh did not finish in time");
    }
    if state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    let epoch = state.session.epoch();
    let result = async { request(state.client().await?).await }.await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(on_error(app, epoch, error).await),
    }
}

/// Whether native translucency is behind the sidebar (macOS vibrancy,
/// Windows 11 Mica). The UI paints a solid sidebar when it is not.
#[tauri::command]
#[specta::specta]
pub async fn window_vibrancy(effects: State<'_, WindowEffects>) -> Result<bool, AppError> {
    Ok(effects.vibrancy)
}

/// Moves the session to `Expired` on an auth error for a request that started
/// at `epoch`, and passes the error on.
async fn on_error(app: &AppHandle, epoch: u64, error: AppError) -> AppError {
    if error.kind == ErrorKind::AuthExpired {
        let state = app.state::<AppState>();
        let mut player = state.player.lock_for_session_change().await;
        let task_app = app.clone();
        match blocking(move || task_app.state::<AppState>().session.mark_expired_if(epoch)).await {
            Ok(Some(status)) => {
                player.stop(app);
                emit_auth_changed(app, status);
            }
            Ok(None) => {}
            Err(task_error) => tracing::warn!(%task_error, "session expiry task failed"),
        }
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_query_trims_and_skips_empty_text() {
        assert_eq!(checked_query("  lofi  ").unwrap(), Some("lofi"));
        assert_eq!(checked_query(" \t ").unwrap(), None);
    }

    #[test]
    fn checked_query_rejects_overlong_text() {
        let limit = "가".repeat(MAX_QUERY_CHARS);
        assert_eq!(checked_query(&limit).unwrap(), Some(limit.as_str()));
        let error = checked_query(&format!("{limit}a")).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidArgument);
    }
}
```

Create `app/src-tauri/src/window_effects.rs`:

```rust
//! Native translucency behind the main window's web content: macOS sidebar
//! vibrancy, Windows 11 Mica. The window is transparent (`tauri.conf.json`)
//! and the page stays opaque except the sidebar, and only when an effect
//! applied, so unsupported systems (Windows 10) show a solid sidebar.

use tauri::WebviewWindow;

/// Managed state: whether `apply` succeeded for the main window.
pub struct WindowEffects {
    pub vibrancy: bool,
}

/// Applies the platform effect to `window`. Call on the main thread (Tauri
/// `setup`). Returns whether the effect is active.
pub fn apply(window: &WebviewWindow) -> bool {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy};
        match apply_vibrancy(
            window,
            NSVisualEffectMaterial::Sidebar,
            Some(NSVisualEffectState::FollowsWindowActiveState),
            None,
        ) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not apply window vibrancy");
                false
            }
        }
    }
    #[cfg(windows)]
    {
        match window_vibrancy::apply_mica(window, Some(true)) {
            Ok(()) => true,
            // Windows 10: Mica needs Windows 11; keep the solid sidebar.
            Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => false,
            Err(error) => {
                tracing::warn!(%error, "could not apply Mica");
                false
            }
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = window;
        false
    }
}
```

- [ ] **Step 3: Register the commands and apply the effect at startup**

In `app/src-tauri/src/lib.rs`:

1. After `pub mod webview_session;` add `pub mod window_effects;`.
2. In `collect_commands![…]`, replace `            commands::get_home,` with:

```rust
            commands::get_home,
            commands::search,
            commands::search_suggestions,
            commands::get_account,
            commands::window_vibrancy,
```

3. In `setup`, replace `            app.manage(AppState::new(session));` with:

```rust
            app.manage(AppState::new(session));
            let vibrancy = app
                .get_webview_window("main")
                .is_some_and(|window| window_effects::apply(&window));
            tracing::info!(vibrancy, "main window effects");
            app.manage(window_effects::WindowEffects { vibrancy });
```

In `app/src-tauri/build.rs`, in the `AppManifest::new().commands(&[…])` list, replace `                "get_home",` with:

```rust
                "get_home",
                "search",
                "search_suggestions",
                "get_account",
                "window_vibrancy",
```

Replace `app/src-tauri/capabilities/default.json` with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Local UI in the main window. The login window has no capability, so its remote pages cannot call any command.",
  "windows": ["main"],
  "permissions": [
    "core:event:default",
    "allow-auth-status",
    "allow-sign-in",
    "allow-sign-out",
    "allow-get-home",
    "allow-search",
    "allow-search-suggestions",
    "allow-get-account",
    "allow-window-vibrancy",
    "allow-play-queue",
    "allow-toggle-playback",
    "allow-next-track",
    "allow-previous-track",
    "allow-seek-to",
    "allow-set-repeat",
    "allow-set-shuffle",
    "allow-player-snapshot"
  ]
}
```

- [ ] **Step 4: Make the main window transparent and dark; widen the image CSP**

In `app/src-tauri/tauri.conf.json`, in the `main` window object, replace

```json
        "minWidth": 800,
        "minHeight": 500
```

with

```json
        "minWidth": 800,
        "minHeight": 500,
        "theme": "Dark",
        "transparent": true
```

and in **both** `csp` and `devCsp` replace `img-src 'self' data: https://i.ytimg.com` with `img-src 'self' data: https://*.ytimg.com`. The resulting `img-src` directive in each is:

```
img-src 'self' data: https://*.ytimg.com https://*.ggpht.com https://*.googleusercontent.com
```

Leave every other directive unchanged. Do not add `windowEffects` or `macOSPrivateApi` (Decision 11, Global Constraints). Until Task 8 ships the new stylesheet, the M3 page paints no background of its own, so a build of this commit shows the material (or, on Windows 10, possibly the desktop) behind the old UI. That is expected; Task 8's stylesheet paints every surface except the sidebar, and the human checks the result after Task 8.

- [ ] **Step 5: Run the tests, regenerate bindings and permissions**

Run: `cargo test -p kaset`
Expected: `116 passed` (114 before plus `checked_query_trims_and_skips_empty_text` and `checked_query_rejects_overlong_text`). The build writes the four `permissions/autogenerated/*.toml` files, and `bindings_are_current` rewrites `app/src/bindings.ts`. Confirm with `git diff --stat app/src/bindings.ts` that it gained `search`, `searchSuggestions`, `getAccount`, `windowVibrancy`, `SearchItem`, `SearchResults`, `Account`, and the four `MockCommandHandlers` entries.

- [ ] **Step 6: Run the remaining checks**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Then (from `app/`): `pnpm typecheck && pnpm lint && pnpm test && pnpm tauri build --bundles app`
Expected: all green; the frontend still has 21 tests (the new commands are not used yet); `Kaset.app` builds (this validates the window config and the capability). The `#[cfg(windows)]` branch of `window_effects::apply` is compiled only by the Windows CI job in Step 7.

- [ ] **Step 7: Commit and push**

```bash
git add Cargo.lock app/src-tauri/Cargo.toml app/src-tauri/src/commands.rs app/src-tauri/src/window_effects.rs app/src-tauri/src/lib.rs app/src-tauri/build.rs app/src-tauri/capabilities/default.json app/src-tauri/tauri.conf.json app/src-tauri/permissions/autogenerated/search.toml app/src-tauri/permissions/autogenerated/search_suggestions.toml app/src-tauri/permissions/autogenerated/get_account.toml app/src-tauri/permissions/autogenerated/window_vibrancy.toml app/src/bindings.ts
git commit -m "feat(app): add search and account commands and native window vibrancy"
git push origin main
gh run list --limit 4
```

Wait for the `App` and `Rust` runs on this commit (`gh run watch <run-id> --exit-status`). Expected: green on macOS and Windows. If the Windows job fails in `window_effects.rs`, fix it against `window-vibrancy` 0.8.1's `src/lib.rs` (`apply_mica(window: impl HasWindowHandle, dark: Option<bool>) -> Result<(), Error>`) without changing the macOS branch.

---
### Task 6: Frontend building blocks — query client, notices, artwork, item cards

Nothing is wired into `App` yet; the existing screens keep working. Everything here is used by Tasks 7–8.

**Files:**
- Modify: `app/package.json`, `app/pnpm-lock.yaml` (via pnpm), `app/src/errors.ts`, `app/src/test/setup.ts`
- Create: `app/src/query.ts`, `app/src/query.test.ts`, `app/src/notices/NoticeProvider.tsx`, `app/src/notices/NoticeProvider.test.tsx`, `app/src/music/items.ts`, `app/src/music/imageUrl.ts`, `app/src/music/imageUrl.test.ts`, `app/src/music/Artwork.tsx`, `app/src/music/Artwork.test.tsx`, `app/src/music/ItemCard.tsx`, `app/src/player/usePlayQueue.ts`, `app/src/test/render.tsx`, `app/src/test/fixtures.ts`

**Interfaces:**
- Consumes: generated `commands.playQueue(songs, startIndex)`, types `Account`, `AppError`, `ArtistRef`, `BrowseCard`, `SearchItem`, `ShelfItem`, `Song`, `HomePage`, `SearchResults`.
- Produces:
  - `query.ts`: `HOME_STALE_MS = 300000`, `SEARCH_STALE_MS = 600000`, `SUGGESTIONS_STALE_MS = 60000`, `ACCOUNT_STALE_MS = 1800000`; `queryKeys.home`, `queryKeys.search(query)`, `queryKeys.suggestions(input)`, `queryKeys.account`; `shouldRetry(failureCount: number, error: unknown): boolean`; `createQueryClient({ retry?: boolean } = {}): QueryClient`.
  - `errors.ts`: `isAppError(value): value is AppError` (now also checks `kind` is a string), `errorMessage(value): string`.
  - `notices/NoticeProvider.tsx`: `<NoticeProvider>`, `useNotice(): { report(message: string): void; dismiss(): void }` (stable identity; throws outside the provider), `<NoticeBanner />` (renders `<div class="notice" role="alert">` with a Dismiss button, or nothing).
  - `music/items.ts`: `type MusicItem = ShelfItem | SearchItem`; `isPlayable<T extends MusicItem>(item: T): item is Extract<T, { video_id: string }>`; `toSong(item: Song): Song`; `artistNames(artists): string`; `subtitleOf(item): string | null`; `itemKey(item): string`; `shapeOf(item): "square" | "round"` (round for `artist`/`profile`).
  - `music/imageUrl.ts`: `safeImageUrl(url: string | null): string | null`.
  - `music/Artwork.tsx`: `<Artwork url={string | null} size={number | "fill"} shape?={"square" | "round"} />` (classes `artwork artwork-<shape>` plus `artwork-fill`; placeholder `artwork-placeholder`, also after a load error).
  - `music/ItemCard.tsx`: `<ItemCard title subtitle thumbnailUrl layout={"card" | "row"} shape?={"square" | "round"} onPlay?={() => void} />` — `card` puts full-width square/round artwork above the text, `row` a 44 px thumbnail beside it; a button labelled `Play <title>[, <subtitle>]` when `onPlay` is set, otherwise a non-interactive `div`.
  - `player/usePlayQueue.ts`: `usePlayQueue(): (songs: Song[], index: number) => Promise<void>` (dismisses the notice, reports failures to it).
  - `test/render.tsx`: `renderWithProviders(ui)` (fresh non-retrying `QueryClient`, `NoticeProvider`, `NoticeBanner`). Task 8 adds `renderApp()`.
  - `test/fixtures.ts`: `home: HomePage` (an all-song "Quick picks", a mixed "Mixed for you" with album/song/artist, a playlist "Listen again" with a disallowed thumbnail), `account: Account`, `searchResults: SearchResults` (artist first, then songs, a video, an album).

- [ ] **Step 1: Add TanStack Query**

From `app/`: `pnpm add @tanstack/react-query@5.107.1`
Expected: `package.json` gains `"@tanstack/react-query": "5.107.1"` (exact) and the lockfile records `5.107.1(@types/react@19.3.0)(react@19.3.1)`. Do not accept a newer version.

- [ ] **Step 2: Write the failing tests**

Create `app/src/query.test.ts`:

```typescript
import { expect, test } from "vitest";
import { createQueryClient, shouldRetry } from "./query";

test("retries network and HTTP failures once", () => {
  expect(shouldRetry(0, { kind: "network", message: "offline" })).toBe(true);
  expect(shouldRetry(0, { kind: "http", message: "HTTP 500" })).toBe(true);
  expect(shouldRetry(1, { kind: "network", message: "offline" })).toBe(false);
});

test("never retries errors a retry cannot fix", () => {
  for (const kind of ["auth_expired", "not_signed_in", "invalid_argument", "parse", "rate_limited", "internal"]) {
    expect(shouldRetry(0, { kind, message: kind })).toBe(false);
  }
  expect(shouldRetry(0, new Error("boom"))).toBe(false);
});

test("the client does not refetch on window focus", () => {
  const client = createQueryClient();
  expect(client.getDefaultOptions().queries?.refetchOnWindowFocus).toBe(false);
  expect(createQueryClient({ retry: false }).getDefaultOptions().queries?.retry).toBe(false);
});
```

Create `app/src/notices/NoticeProvider.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { NoticeBanner, NoticeProvider, useNotice } from "./NoticeProvider";

function Reporter() {
  const { report } = useNotice();
  return (
    <>
      <button type="button" onClick={() => report("first failure")}>
        First
      </button>
      <button type="button" onClick={() => report("second failure")}>
        Second
      </button>
    </>
  );
}

test("shows the latest notice once and dismisses it", async () => {
  render(
    <NoticeProvider>
      <NoticeBanner />
      <Reporter />
    </NoticeProvider>,
  );
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "First" }));
  await userEvent.click(screen.getByRole("button", { name: "Second" }));
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(screen.getByRole("alert")).toHaveTextContent("second failure");

  await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("useNotice outside the provider fails loudly", () => {
  expect(() => render(<Reporter />)).toThrow("useNotice must be used inside <NoticeProvider>");
});
```

Create `app/src/music/imageUrl.test.ts`:

```typescript
import { expect, test } from "vitest";
import { safeImageUrl } from "./imageUrl";

test("keeps https artwork on YouTube image hosts", () => {
  for (const url of [
    "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
    "https://i9.ytimg.com/vi/test-vid-01/sddefault.jpg",
    "https://lh3.googleusercontent.com/test=w120-h120",
    "https://yt3.googleusercontent.com/test=s240",
    "https://yt3.ggpht.com/test=s88",
  ]) {
    expect(safeImageUrl(url)).toBe(url);
  }
});

test("upgrades protocol-relative URLs", () => {
  expect(safeImageUrl("//i.ytimg.com/vi/test/default.jpg")).toBe("https://i.ytimg.com/vi/test/default.jpg");
});

test("rejects other schemes, hosts, and junk", () => {
  for (const url of [
    null,
    "http://i.ytimg.com/vi/test/default.jpg",
    "https://example.com/a.jpg",
    "https://ytimg.com.example.com/a.jpg",
    "https://googleusercontent.com/a.jpg",
    "javascript:alert(1)",
    "not a url",
  ]) {
    expect(safeImageUrl(url)).toBe(null);
  }
});
```

Create `app/src/music/Artwork.test.tsx`:

```tsx
import { fireEvent, render } from "@testing-library/react";
import { expect, test } from "vitest";
import { Artwork } from "./Artwork";

test("shows allowed artwork and falls back to a placeholder when it fails", () => {
  const { container } = render(<Artwork url="https://i.ytimg.com/vi/test/default.jpg" size={48} />);
  const img = container.querySelector("img");
  expect(img).toHaveAttribute("src", "https://i.ytimg.com/vi/test/default.jpg");
  expect(img).toHaveAttribute("alt", "");
  fireEvent.error(img as HTMLImageElement);
  expect(container.querySelector("img")).toBeNull();
  expect(container.querySelector(".artwork-placeholder")).not.toBeNull();
});

test("never requests a disallowed URL", () => {
  const { container } = render(<Artwork url="http://example.com/a.jpg" size={48} />);
  expect(container.querySelector("img")).toBeNull();
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run (from `app/`): `pnpm vitest run src/query.test.ts src/notices src/music`
Expected: FAIL — the imported modules do not exist.

- [ ] **Step 4: Implement the building blocks**

Replace `app/src/errors.ts` with:

```typescript
import type { AppError } from "./bindings";

/** Commands reject with the Rust `AppError` (`{ kind, message }`). */
export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    typeof value.kind === "string" &&
    "message" in value &&
    typeof value.message === "string"
  );
}

export function errorMessage(value: unknown): string {
  if (isAppError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}
```

Create `app/src/query.ts`:

```typescript
import { QueryClient } from "@tanstack/react-query";
import { isAppError } from "./errors";

/** How long fetched data counts as fresh before a remount refetches it. */
export const HOME_STALE_MS = 5 * 60_000;
export const SEARCH_STALE_MS = 10 * 60_000;
export const SUGGESTIONS_STALE_MS = 60_000;
export const ACCOUNT_STALE_MS = 30 * 60_000;

export const queryKeys = {
  home: ["home"] as const,
  search: (query: string) => ["search", query] as const,
  suggestions: (input: string) => ["suggestions", input] as const,
  account: ["account"] as const,
};

/**
 * Retries once after a network failure or an unexpected HTTP status. Auth,
 * rate-limit, parse, and argument errors fail at once: retrying cannot fix
 * them, and an auth error already switched the app to the sign-in screen.
 */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  return failureCount < 1 && isAppError(error) && (error.kind === "network" || error.kind === "http");
}

export function createQueryClient({ retry = true }: { retry?: boolean } = {}): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: retry ? shouldRetry : false,
        // A desktop window gains focus constantly; refetch on remount or Refresh instead.
        refetchOnWindowFocus: false,
      },
    },
  });
}
```

Create `app/src/notices/NoticeProvider.tsx`:

```tsx
import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

export type NoticeApi = {
  /** Shows `message` in the app's single notice area, replacing any earlier one. */
  report: (message: string) => void;
  dismiss: () => void;
};

const NoticeContext = createContext<NoticeApi | null>(null);
const MessageContext = createContext<string | null>(null);

/**
 * One app-wide place for action errors (play, sign out, player controls), so
 * the same failure is never shown twice. Screens keep their own load errors.
 */
export function NoticeProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<string | null>(null);
  const api = useMemo<NoticeApi>(
    () => ({ report: setMessage, dismiss: () => setMessage(null) }),
    [],
  );
  return (
    <NoticeContext value={api}>
      <MessageContext value={message}>{children}</MessageContext>
    </NoticeContext>
  );
}

export function useNotice(): NoticeApi {
  const api = useContext(NoticeContext);
  if (api === null) throw new Error("useNotice must be used inside <NoticeProvider>");
  return api;
}

export function NoticeBanner() {
  const message = useContext(MessageContext);
  const { dismiss } = useNotice();
  if (message === null) return null;
  return (
    <div className="notice" role="alert">
      <p>{message}</p>
      <button type="button" onClick={dismiss}>
        Dismiss
      </button>
    </div>
  );
}
```

Create `app/src/music/items.ts`:

```typescript
import type { ArtistRef, BrowseCard, SearchItem, ShelfItem, Song } from "../bindings";

/** A home or search item: a song-like item or a browse card, tagged by `kind`. */
export type MusicItem = ShelfItem | SearchItem;

/** Items that carry a video id and can be queued. */
export function isPlayable<T extends MusicItem>(item: T): item is Extract<T, { video_id: string }> {
  return "video_id" in item;
}

/** The `Song` that `playQueue` takes, without the `kind` tag. */
export function toSong(item: Song): Song {
  return {
    video_id: item.video_id,
    title: item.title,
    artists: item.artists,
    thumbnail_url: item.thumbnail_url,
  };
}

export function artistNames(artists: ArtistRef[]): string {
  return artists.map((artist) => artist.name).join(", ");
}

/** The second line under an item's title. */
export function subtitleOf(item: MusicItem): string | null {
  if (isPlayable(item)) return artistNames(item.artists) || null;
  return (item as BrowseCard).subtitle;
}

/** A stable React key for an item within its list. */
export function itemKey(item: MusicItem): string {
  return isPlayable(item) ? `${item.kind}:${item.video_id}` : `${item.kind}:${item.browse_id}`;
}

/** Artists and profiles use round artwork. */
export function shapeOf(item: MusicItem): "square" | "round" {
  return item.kind === "artist" || item.kind === "profile" ? "round" : "square";
}
```

Create `app/src/music/imageUrl.ts`:

```typescript
/** Hosts YouTube Music serves artwork from; `tauri.conf.json`'s CSP allows the same. */
const ARTWORK_HOST = /\.(ytimg|ggpht|googleusercontent)\.com$/;

/**
 * The artwork URL if it is https on an allowed host (protocol-relative URLs
 * become https); otherwise `null`, so the UI shows a placeholder instead of
 * an image the CSP would block.
 */
export function safeImageUrl(url: string | null): string | null {
  if (url === null) return null;
  let parsed: URL;
  try {
    parsed = new URL(url.startsWith("//") ? `https:${url}` : url);
  } catch {
    return null;
  }
  return parsed.protocol === "https:" && ARTWORK_HOST.test(parsed.hostname) ? parsed.href : null;
}
```

Create `app/src/music/Artwork.tsx`:

```tsx
import { useState } from "react";
import { safeImageUrl } from "./imageUrl";

type Props = {
  url: string | null;
  /** Width and height in CSS pixels, or `fill` to fill the parent's width (square). */
  size: number | "fill";
  /** Artists use round artwork; everything else is square. */
  shape?: "square" | "round";
};

/**
 * Square or round artwork, or a neutral placeholder when there is no allowed
 * URL or the image fails to load. Decorative: titles carry the meaning.
 */
export function Artwork({ url, size, shape = "square" }: Props) {
  const src = safeImageUrl(url);
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const className = `artwork artwork-${shape}${size === "fill" ? " artwork-fill" : ""}`;
  const dimensions = size === "fill" ? undefined : { width: size, height: size };
  if (src === null || src === failedSrc) {
    return <span className={`${className} artwork-placeholder`} style={dimensions} aria-hidden="true" />;
  }
  return (
    <img
      className={className}
      src={src}
      {...dimensions}
      alt=""
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      onError={() => setFailedSrc(src)}
    />
  );
}
```

Create `app/src/music/ItemCard.tsx`:

```tsx
import { Artwork } from "./Artwork";

type Props = {
  title: string;
  subtitle: string | null;
  thumbnailUrl: string | null;
  /** `card`: artwork above the text (carousels); `row`: 44 px artwork beside it (lists). */
  layout: "card" | "row";
  /** Artists are round. */
  shape?: "square" | "round";
  /** Plays the item; without it the item is shown but not interactive. */
  onPlay?: () => void;
};

export function ItemCard({ title, subtitle, thumbnailUrl, layout, shape = "square", onPlay }: Props) {
  const body = (
    <>
      <Artwork url={thumbnailUrl} size={layout === "card" ? "fill" : 44} shape={shape} />
      <span className="item-text">
        <span className="item-title">{title}</span>
        {subtitle && <span className="item-subtitle">{subtitle}</span>}
      </span>
    </>
  );
  const className = `item item-${layout}`;
  if (onPlay === undefined) return <div className={className}>{body}</div>;
  return (
    <button
      type="button"
      className={className}
      aria-label={subtitle ? `Play ${title}, ${subtitle}` : `Play ${title}`}
      onClick={onPlay}
    >
      {body}
    </button>
  );
}
```

Create `app/src/player/usePlayQueue.ts`:

```typescript
import { useCallback } from "react";
import { commands, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { useNotice } from "../notices/NoticeProvider";

/** Plays `songs` from `songs[index]`; a failure goes to the app notice. */
export function usePlayQueue(): (songs: Song[], index: number) => Promise<void> {
  const { report, dismiss } = useNotice();
  return useCallback(
    async (songs: Song[], index: number) => {
      dismiss();
      try {
        await commands.playQueue(songs, index);
      } catch (cause) {
        report(errorMessage(cause));
      }
    },
    [report, dismiss],
  );
}
```

- [ ] **Step 5: Add the shared test helpers**

Replace `app/src/test/setup.ts` with (adds `vi.restoreAllMocks()` so spies on the generated `commands` / `events` objects cannot leak between tests):

```typescript
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, vi } from "vitest";

afterEach(async () => {
  cleanup();
  // Let effect cleanups (event unlisten promises) settle while the IPC mock
  // still exists, then remove it.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  // Spies on the generated `commands` / `events` objects must not leak into the next test.
  vi.restoreAllMocks();
});
```

Create `app/src/test/render.tsx`:

```tsx
import { QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { NoticeBanner, NoticeProvider } from "../notices/NoticeProvider";
import { createQueryClient } from "../query";

/** `ui` inside the providers the shell gives its screens, with the notice banner. */
export function renderWithProviders(ui: ReactElement) {
  return render(
    <QueryClientProvider client={createQueryClient({ retry: false })}>
      <NoticeProvider>
        <NoticeBanner />
        {ui}
      </NoticeProvider>
    </QueryClientProvider>,
  );
}
```

Create `app/src/test/fixtures.ts`:

```typescript
import type { Account, HomePage, SearchResults } from "../bindings";

export const home: HomePage = {
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
```

- [ ] **Step 6: Run the checks**

Run (from `app/`): `pnpm test && pnpm lint && pnpm typecheck`
Expected: `Tests  31 passed (31)` in 7 files; lint and typecheck clean.

- [ ] **Step 7: Commit and push**

```bash
git add app/package.json app/pnpm-lock.yaml app/src/errors.ts app/src/query.ts app/src/query.test.ts app/src/notices/ app/src/music/ app/src/player/usePlayQueue.ts app/src/test/setup.ts app/src/test/render.tsx app/src/test/fixtures.ts
git commit -m "feat(app): add query client, notices, and artwork building blocks"
git push origin main
```

---

### Task 7: Search screen — header field, suggestions, chips, top result

The screen is built and tested on its own; Task 8 puts it in the shell. The search field sits in the sticky content header (`<header class="content-header">`), the suggestions dropdown under it; below, filter chips, the top-result card, and grouped results (Decision 9).

**Files:**
- Create: `app/src/search/useDebouncedValue.ts`, `app/src/search/groups.ts`, `app/src/search/SearchBox.tsx`, `app/src/search/SearchScreen.tsx`, `app/src/search/SearchScreen.test.tsx`

**Interfaces:**
- Consumes: `commands.search(query)`, `commands.searchSuggestions(input)` (Task 5); `queryKeys`, `SEARCH_STALE_MS`, `SUGGESTIONS_STALE_MS`; `Artwork`, `ItemCard`, `isPlayable`, `itemKey`, `shapeOf`, `subtitleOf`, `toSong`, `usePlayQueue`, `errorMessage` (Task 6); `renderWithProviders`, `searchResults` (Task 6 test helpers).
- Produces: `<SearchScreen query={string} onSubmit={(query: string) => void} focusRequest={number} />` (the submitted query lives in the caller); `<SearchBox initialText onSubmit focusRequest />` (combobox labelled "Search YouTube Music", listbox labelled "Suggestions"); `SUGGESTION_DELAY_MS = 250`; `useDebouncedValue<T>(value: T, delayMs: number): T`; `groupResults(items: SearchItem[]): ResultGroup[]` with `ResultGroup = { title: string; items: SearchItem[]; playable: boolean }`, fixed order Songs, Videos, Albums, Artists (artist + profile), Playlists, Podcasts, Episodes, empty groups omitted; `type Filter = "all" | "songs" | "albums" | "artists" | "playlists"`, `FILTERS` (chip value, label, group title), `KIND_LABEL` (display name per `SearchItem` kind). The screen renders a visually hidden `<h1>Search</h1>`, chips in `role="group"` labelled "Filter results" with `aria-pressed`, a "Top result" region (Play button only for playable items, which plays that item alone), and one region per group; a new search resets the chip to All.

- [ ] **Step 1: Write the failing tests**

Create `app/src/search/SearchScreen.test.tsx`:

```tsx
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { searchResults } from "../test/fixtures";
import { renderWithProviders } from "../test/render";
import { SearchScreen } from "./SearchScreen";

/** SearchScreen with the query state the shell normally keeps. */
function Harness({ initial = "" }: { initial?: string }) {
  const [query, setQuery] = useState(initial);
  return <SearchScreen query={query} onSubmit={setQuery} focusRequest={0} />;
}

function field() {
  return screen.getByRole("combobox", { name: "Search YouTube Music" });
}

test("submitting shows the top result, then results grouped by type", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ search, searchSuggestions: () => [] });
  renderWithProviders(<Harness />);

  expect(field()).toHaveFocus();
  await userEvent.type(field(), "  test query {Enter}");
  expect(search).toHaveBeenCalledWith({ query: "test query" });

  const headings = await screen.findAllByRole("heading", { level: 2 });
  expect(headings.map((heading) => heading.textContent)).toEqual(["Top result", "Songs", "Videos", "Albums"]);
  const top = screen.getByRole("region", { name: "Top result" });
  expect(within(top).getByText("Test artist")).toBeInTheDocument();
  expect(within(top).getByText("Artist • 42K subscribers")).toBeInTheDocument();
  // An artist top result opens in M5; nothing to play yet.
  expect(within(top).queryByRole("button")).not.toBeInTheDocument();

  const songs = screen.getByRole("region", { name: "Songs" });
  expect(within(songs).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
    "Play Found song, Test artist",
    "Play Another song",
  ]);
  const albums = screen.getByRole("region", { name: "Albums" });
  expect(within(albums).queryByRole("button")).not.toBeInTheDocument();
  expect(within(albums).getByText("Test artist • 2024")).toBeInTheDocument();
});

test("filter chips narrow the loaded results without a new request", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ search, searchSuggestions: () => [] });
  renderWithProviders(<Harness initial="test query" />);

  const chips = await screen.findByRole("group", { name: "Filter results" });
  expect(within(chips).getAllByRole("button").map((chip) => chip.textContent)).toEqual([
    "All",
    "Songs",
    "Albums",
    "Artists",
    "Playlists",
  ]);
  expect(within(chips).getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "true");

  await userEvent.click(within(chips).getByRole("button", { name: "Albums" }));
  expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual(["Albums"]);
  await userEvent.click(within(chips).getByRole("button", { name: "Artists" }));
  expect(screen.getByRole("region", { name: "Artists" })).toHaveTextContent("Test artist");
  await userEvent.click(within(chips).getByRole("button", { name: "Playlists" }));
  expect(screen.getByText("No playlists in these results.")).toBeInTheDocument();
  expect(search).toHaveBeenCalledTimes(1);
});

test("a playable top result plays on its own", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({ search: () => ({ items: searchResults.items.slice(1), continuation: null }), playQueue });
  renderWithProviders(<Harness initial="test query" />);

  const top = await screen.findByRole("region", { name: "Top result" });
  await userEvent.click(within(top).getByRole("button", { name: "Play" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [
      {
        video_id: "test-vid-11",
        title: "Found song",
        artists: [{ name: "Test artist", browse_id: "UCtest-artist" }],
        thumbnail_url: null,
      },
    ],
    startIndex: 0,
  });
});

test("clicking a song queues that group's songs", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({ search: () => searchResults, playQueue });
  renderWithProviders(<Harness initial="test query" />);

  await userEvent.click(await screen.findByRole("button", { name: "Play Another song" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [
      {
        video_id: "test-vid-11",
        title: "Found song",
        artists: [{ name: "Test artist", browse_id: "UCtest-artist" }],
        thumbnail_url: null,
      },
      { video_id: "test-vid-13", title: "Another song", artists: [], thumbnail_url: null },
    ],
    startIndex: 1,
  });
});

test("suggestions are debounced and selectable with the keyboard", async () => {
  const searchSuggestions = vi.fn(({ input }: { input: string }) => [`${input} live`, `${input} remix`]);
  const search = vi.fn(() => searchResults);
  mockCommands({ searchSuggestions, search });
  renderWithProviders(<Harness />);

  await userEvent.type(field(), "lofi");
  const listbox = await screen.findByRole("listbox", { name: "Suggestions" });
  expect(within(listbox).getAllByRole("option").map((option) => option.textContent)).toEqual([
    "lofi live",
    "lofi remix",
  ]);
  // One request for the settled text, not one per keystroke.
  expect(searchSuggestions).toHaveBeenCalledTimes(1);
  expect(searchSuggestions).toHaveBeenCalledWith({ input: "lofi" });
  expect(field()).toHaveAttribute("aria-expanded", "true");

  await userEvent.keyboard("{ArrowDown}{ArrowDown}");
  const remix = screen.getByRole("option", { name: "lofi remix" });
  expect(remix).toHaveAttribute("aria-selected", "true");
  expect(field()).toHaveAttribute("aria-activedescendant", remix.id);

  await userEvent.keyboard("{Enter}");
  expect(search).toHaveBeenCalledWith({ query: "lofi remix" });
  expect(field()).toHaveValue("lofi remix");
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
});

test("clicking a suggestion searches for it and Escape closes the list", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ searchSuggestions: () => ["first idea", "second idea"], search });
  renderWithProviders(<Harness />);

  await userEvent.type(field(), "idea");
  await screen.findByRole("listbox");
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();

  await userEvent.type(field(), "s");
  await userEvent.click(await screen.findByRole("option", { name: "second idea" }));
  expect(search).toHaveBeenCalledWith({ query: "second idea" });
});

test("no results and errors are explained", async () => {
  let calls = 0;
  mockCommands({
    searchSuggestions: () => [],
    search: () => {
      calls += 1;
      if (calls === 1) throw { kind: "network", message: "network error" };
      return { items: [], continuation: null };
    },
  });
  renderWithProviders(<Harness initial="nothing" />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network error");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("No results for “nothing”.")).toBeInTheDocument();
});

test("an empty query does not search", async () => {
  const search = vi.fn(() => searchResults);
  mockCommands({ search, searchSuggestions: () => [] });
  renderWithProviders(<Harness />);
  await userEvent.type(field(), "   {Enter}");
  expect(search).not.toHaveBeenCalled();
  expect(screen.getByText(/search for songs/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/`): `pnpm vitest run src/search`
Expected: FAIL — `./SearchScreen` does not exist.

- [ ] **Step 3: Implement the screen**

Create `app/src/search/useDebouncedValue.ts`:

```typescript
import { useEffect, useState } from "react";

/** `value`, updated only after it has stayed the same for `delayMs`. */
export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}
```

Create `app/src/search/groups.ts`:

```typescript
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
```

Create `app/src/search/SearchBox.tsx`:

```tsx
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { commands } from "../bindings";
import { queryKeys, SUGGESTIONS_STALE_MS } from "../query";
import { useDebouncedValue } from "./useDebouncedValue";

/** Delay between the last keystroke and the suggestions request. */
export const SUGGESTION_DELAY_MS = 250;

type Props = {
  initialText: string;
  onSubmit: (query: string) => void;
  /** Focuses the field on mount and whenever this number changes. */
  focusRequest: number;
};

/** A search field with autocomplete (ARIA combobox with a listbox popup). */
export function SearchBox({ initialText, onSubmit, focusRequest }: Props) {
  const [text, setText] = useState(initialText);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  const debouncedText = useDebouncedValue(text.trim(), SUGGESTION_DELAY_MS);

  const suggestions = useQuery({
    queryKey: queryKeys.suggestions(debouncedText),
    queryFn: () => commands.searchSuggestions(debouncedText),
    enabled: open && debouncedText !== "",
    staleTime: SUGGESTIONS_STALE_MS,
    placeholderData: keepPreviousData,
  });
  // Suggestion failures are not worth an alert: the user can still press Return.
  const options = open && text.trim() !== "" ? (suggestions.data ?? []) : [];
  const activeIndex = active < options.length ? active : -1;

  useEffect(() => {
    input.current?.focus();
  }, [focusRequest]);

  function submit(value: string) {
    const query = value.trim();
    if (query === "") return;
    setText(query);
    setOpen(false);
    setActive(-1);
    onSubmit(query);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setOpen(true);
        setActive(Math.min(activeIndex + 1, options.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive(Math.max(activeIndex - 1, -1));
        break;
      case "Enter":
        if (activeIndex >= 0) {
          event.preventDefault();
          submit(options[activeIndex]);
        }
        break;
      case "Escape":
        if (options.length > 0) {
          event.preventDefault();
          setOpen(false);
          setActive(-1);
        }
        break;
    }
  }

  return (
    <form
      role="search"
      className="search-box"
      onSubmit={(event) => {
        event.preventDefault();
        submit(text);
      }}
    >
      <input
        ref={input}
        type="search"
        role="combobox"
        aria-label="Search YouTube Music"
        aria-autocomplete="list"
        aria-expanded={options.length > 0}
        aria-controls={listId}
        aria-activedescendant={activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined}
        placeholder="Songs, albums, artists"
        autoComplete="off"
        spellCheck={false}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setOpen(true);
          setActive(-1);
        }}
        onKeyDown={onKeyDown}
        onBlur={() => setOpen(false)}
      />
      <ul id={listId} role="listbox" aria-label="Suggestions" className="suggestions" hidden={options.length === 0}>
        {options.map((option, index) => (
          <li
            key={option}
            id={`${listId}-${index}`}
            role="option"
            aria-selected={index === activeIndex}
            // Keep focus in the field so the click lands before blur closes the list.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => submit(option)}
          >
            {option}
          </li>
        ))}
      </ul>
    </form>
  );
}
```

Create `app/src/search/SearchScreen.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { commands, type SearchItem } from "../bindings";
import { errorMessage } from "../errors";
import { Artwork } from "../music/Artwork";
import { ItemCard } from "../music/ItemCard";
import { isPlayable, itemKey, shapeOf, subtitleOf, toSong } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { queryKeys, SEARCH_STALE_MS } from "../query";
import { FILTERS, groupResults, KIND_LABEL, type Filter, type ResultGroup } from "./groups";
import { SearchBox } from "./SearchBox";

type Props = {
  /** The submitted query; empty before the first search. */
  query: string;
  onSubmit: (query: string) => void;
  focusRequest: number;
};

export function SearchScreen({ query, onSubmit, focusRequest }: Props) {
  const [filter, setFilter] = useState<Filter>("all");
  const results = useQuery({
    queryKey: queryKeys.search(query),
    queryFn: () => commands.search(query),
    enabled: query !== "",
    staleTime: SEARCH_STALE_MS,
  });
  const play = usePlayQueue();
  const items = results.data?.items ?? [];

  function submit(next: string) {
    setFilter("all");
    onSubmit(next);
  }

  return (
    <>
      <header className="content-header">
        <SearchBox initialText={query} onSubmit={submit} focusRequest={focusRequest} />
      </header>
      <h1 className="visually-hidden">Search</h1>
      {query === "" && <p className="hint">Search for songs, videos, albums, artists, and playlists.</p>}
      {results.isLoading && <p className="hint">Searching…</p>}
      {results.isError && (
        <div role="alert" className="screen-error">
          <p>{errorMessage(results.error)}</p>
          <button type="button" onClick={() => void results.refetch()}>
            Retry
          </button>
        </div>
      )}
      {results.data && items.length === 0 && <p className="hint">No results for “{query}”.</p>}
      {items.length > 0 && (
        <>
          <div className="chips" role="group" aria-label="Filter results">
            {FILTERS.map((chip) => (
              <button
                key={chip.value}
                type="button"
                className="chip"
                aria-pressed={filter === chip.value}
                onClick={() => setFilter(chip.value)}
              >
                {chip.label}
              </button>
            ))}
          </div>
          {filter === "all" ? (
            <AllResults items={items} onPlay={play} />
          ) : (
            <FilteredResults items={items} filter={filter} onPlay={play} />
          )}
        </>
      )}
    </>
  );
}

type PlayFn = ReturnType<typeof usePlayQueue>;

/** The top result as a large card, then every group without it. */
function AllResults({ items, onPlay }: { items: SearchItem[]; onPlay: PlayFn }) {
  const [top, ...rest] = items;
  return (
    <>
      <TopResult item={top} onPlay={onPlay} />
      {groupResults(rest).map((group) => (
        <Group key={group.title} group={group} onPlay={onPlay} />
      ))}
    </>
  );
}

function FilteredResults({ items, filter, onPlay }: { items: SearchItem[]; filter: Filter; onPlay: PlayFn }) {
  const title = FILTERS.find((chip) => chip.value === filter)?.group;
  const group = groupResults(items).find((candidate) => candidate.title === title);
  if (group === undefined) return <p className="hint">No {title?.toLowerCase()} in these results.</p>;
  return <Group group={group} onPlay={onPlay} />;
}

function TopResult({ item, onPlay }: { item: SearchItem; onPlay: PlayFn }) {
  const headingId = useId();
  const subtitle = subtitleOf(item);
  return (
    <section className="top-result" aria-labelledby={headingId}>
      <h2 id={headingId}>Top result</h2>
      <div className="top-card">
        <Artwork url={item.thumbnail_url} size={96} shape={shapeOf(item)} />
        <div className="top-text">
          <p className="top-title">{item.title}</p>
          <p className="item-subtitle">{[KIND_LABEL[item.kind], subtitle].filter(Boolean).join(" • ")}</p>
          {isPlayable(item) && (
            <button type="button" className="primary" onClick={() => void onPlay([toSong(item)], 0)}>
              Play
            </button>
          )}
        </div>
      </div>
    </section>
  );
}

function Group({ group, onPlay }: { group: ResultGroup; onPlay: PlayFn }) {
  const headingId = useId();
  const songs = group.playable ? group.items.filter(isPlayable).map(toSong) : [];
  return (
    <section className="result-group" aria-labelledby={headingId}>
      <h2 id={headingId}>{group.title}</h2>
      <ul className="rows">
        {group.items.map((item, index) => (
          <li key={itemKey(item)}>
            <ItemCard
              layout="row"
              shape={shapeOf(item)}
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onPlay={group.playable && isPlayable(item) ? () => void onPlay(songs, index) : undefined}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}
```

- [ ] **Step 4: Run the checks**

Run (from `app/`): `pnpm test && pnpm lint && pnpm typecheck`
Expected: `Tests  39 passed (39)` in 8 files; lint and typecheck clean.

- [ ] **Step 5: Commit and push**

```bash
git add app/src/search/
git commit -m "feat(app): add search screen with suggestions, filter chips, and top result"
git push origin main
```

---

### Task 8: App shell — dark theme, sidebar, home grid and carousels, floating player bar, shortcuts

**Files:**
- Modify: `app/src/main.tsx`, `app/src/App.tsx`, `app/src/App.test.tsx`, `app/src/auth/useAuthStatus.ts`, `app/src/auth/SignInScreen.tsx`, `app/src/auth/SignInScreen.test.tsx`, `app/src/home/HomeScreen.tsx`, `app/src/player/usePlayer.ts`, `app/src/player/PlayerBar.tsx`, `app/src/player/PlayerBar.test.tsx`, `app/src/styles.css`, `app/src/test/render.tsx`, `docs/keyboard-shortcuts.md`
- Create: `app/src/auth/useAuthStatus.test.tsx`, `app/src/shell/Shell.tsx`, `app/src/shell/Sidebar.tsx`, `app/src/shell/shortcuts.ts`, `app/src/shell/shortcuts.test.ts`, `app/src/shell/useShortcuts.ts`, `app/src/home/HomeScreen.test.tsx`

**Interfaces:**
- Consumes: everything from Task 6 and `SearchScreen` (Task 7); generated `commands.{authStatus, signOut, getHome, getAccount, windowVibrancy, togglePlayback, nextTrack, previousTrack, seekTo, setRepeat, setShuffle, playerSnapshot}`, `events.{authChanged, playerState, playerQueue}`; existing `SignInScreen`, `formatTime`, `player/testing.ts` (`testSong`, `idleSnapshot`, `playingState`).
- Produces:
  - `App({ queryClient }: { queryClient: QueryClient })` — provides the client, shows Loading / an auth error with "Try again" / `SignInScreen` / `Shell`, and clears the query cache whenever the status is not `signed_in`.
  - `useAuthStatus(): { status: AuthStatus | null; error: string | null; retry(): void }`.
  - `main.tsx` — awaits `commands.windowVibrancy()` (false on failure) and sets `document.documentElement.dataset.vibrancy` to `"native"` or `"none"` before rendering.
  - `Shell()` — `NoticeProvider` around a grid of `Sidebar` (`<nav aria-label="Main">`: Search and Home buttons with `aria-current="page"` on the active one, an "Account" region with photo, name — "Signed in" if `getAccount` fails or returns null — handle, and Sign out) and a `content-column` holding `<main class="content">` (the current screen, whose first child is its sticky `content-header`), the floating `NoticeBanner`, and the floating `PlayerBar`. `type Route = "home" | "search"` (exported from `Sidebar.tsx`).
  - `SignInScreen({ expired })` — an expired session renders a `role="alert"` box.
  - `shortcutFor(event, mac = isMac()): Shortcut | null` with `Shortcut = "togglePlayback" | "nextTrack" | "previousTrack" | "goHome" | "goSearch"`; `isMac()`; `useShortcuts(actions: Record<Shortcut, () => void>)`.
  - `HomeScreen()` — TanStack Query `queryKeys.home`, header with Refresh; the first all-song shelf as `ul.song-grid` (three columns of `row` items), every other shelf as `ul.carousel` (six `card` items per view) with buttons labelled `Previous in <title>` / `Next in <title>` calling `scrollBy({ left: ∓clientWidth, behavior: "smooth" })`.
  - `usePlayer(onError: (message: string) => void): { state, queue }` (pass a stable function); `PlayerBar()` (needs `NoticeProvider`): Previous / Play-Pause / Next on the left, artwork + title + seek in the middle, Shuffle / Repeat / Queue (`aria-expanded`, toggles a "Queue" region listing the songs with `aria-current="true"` on the current one) on the right; publishes its height as `--player-space` on its parent element through a `ResizeObserver` (skipped when `ResizeObserver` is undefined) and removes it on unmount.
  - `test/render.tsx`: adds `renderApp()`.

- [ ] **Step 1: Write the failing tests**

Replace `app/src/test/render.tsx` with (adds `renderApp`):

```tsx
import { QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { App } from "../App";
import { NoticeBanner, NoticeProvider } from "../notices/NoticeProvider";
import { createQueryClient } from "../query";

/** The whole app with a fresh query cache that never retries. */
export function renderApp() {
  return render(<App queryClient={createQueryClient({ retry: false })} />);
}

/** `ui` inside the providers the shell gives its screens, with the notice banner. */
export function renderWithProviders(ui: ReactElement) {
  return render(
    <QueryClientProvider client={createQueryClient({ retry: false })}>
      <NoticeProvider>
        <NoticeBanner />
        {ui}
      </NoticeProvider>
    </QueryClientProvider>,
  );
}
```

Replace `app/src/App.test.tsx` with:

```tsx
import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "./bindings";
import { idleSnapshot } from "./player/testing";
import { account, home } from "./test/fixtures";
import { renderApp } from "./test/render";

test("signed out shows the sign-in screen", async () => {
  mockCommands({ authStatus: () => "signed_out" });
  renderApp();
  expect(await screen.findByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("signed in shows the shell with the home feed and the account", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
  const nav = screen.getByRole("navigation", { name: "Main" });
  expect(within(nav).getAllByRole("button").map((button) => button.textContent)).toEqual([
    "Search",
    "Home",
    "Sign out",
  ]);
  expect(within(nav).getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
  const accountBlock = within(nav).getByRole("region", { name: "Account" });
  expect(await within(accountBlock).findByText("Test User")).toBeInTheDocument();
  expect(within(accountBlock).getByText("@testuser")).toBeInTheDocument();
});

test("an account failure falls back to a generic label", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    getAccount: () => {
      throw { kind: "parse", message: "could not parse" };
    },
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  const accountBlock = await screen.findByRole("region", { name: "Account" });
  expect(within(accountBlock).getByText("Signed in")).toBeInTheDocument();
  await screen.findByRole("heading", { name: "Quick picks" });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("auth://changed switches screens", async () => {
  mockCommands({
    authStatus: () => "signed_out",
    getHome: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  await screen.findByRole("button", { name: "Sign in with Google" });

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();
});

test("an auth_expired home error routes to sign-in and a new sign-in refetches", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => "signed_in",
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    getHome: () => {
      calls += 1;
      if (calls === 1) throw { kind: "auth_expired", message: "authentication expired" };
      return home;
    },
  });
  renderApp();
  await vi.waitFor(() => expect(calls).toBe(1));
  // Rust emits auth://changed alongside the auth_expired rejection.
  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
  expect(calls).toBe(2);
});

test("an authStatus failure is shown with a retry", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => {
      calls += 1;
      if (calls === 1) throw { kind: "internal", message: "store unavailable" };
      return "signed_out";
    },
  });
  renderApp();
  expect(await screen.findByRole("alert")).toHaveTextContent("store unavailable");
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(await screen.findByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("sign out calls the command", async () => {
  const signOut = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    signOut,
  });
  renderApp();
  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(signOut).toHaveBeenCalledTimes(1);
});

test("a sign-out failure is a notice and the feed stays", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    signOut: () => {
      throw { kind: "internal", message: "keychain denied" };
    },
  });
  renderApp();
  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not sign out: keychain denied");
  expect(screen.getByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
});

test("the sidebar switches between Home and Search", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
  });
  renderApp();
  await screen.findByRole("heading", { name: "Quick picks" });
  await userEvent.click(screen.getByRole("button", { name: "Search" }));
  expect(screen.getByRole("heading", { level: 1, name: "Search" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Search" })).toHaveAttribute("aria-current", "page");
  expect(screen.getByRole("combobox", { name: "Search YouTube Music" })).toHaveFocus();
  await userEvent.click(screen.getByRole("button", { name: "Home" }));
  expect(await screen.findByRole("heading", { name: "Quick picks" })).toBeInTheDocument();
});

test("keyboard shortcuts control playback and navigation", async () => {
  const togglePlayback = vi.fn(() => null);
  const nextTrack = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    getAccount: () => account,
    playerSnapshot: () => idleSnapshot,
    togglePlayback,
    nextTrack,
  });
  renderApp();
  await screen.findByRole("heading", { name: "Quick picks" });

  await userEvent.keyboard(" ");
  expect(togglePlayback).toHaveBeenCalledTimes(1);
  await userEvent.keyboard("{Control>}{ArrowRight}{/Control}");
  expect(nextTrack).toHaveBeenCalledTimes(1);
  await userEvent.keyboard("{Control>}f{/Control}");
  expect(screen.getByRole("combobox", { name: "Search YouTube Music" })).toHaveFocus();
  // Space types in the search field instead of toggling playback.
  await userEvent.keyboard(" ");
  expect(togglePlayback).toHaveBeenCalledTimes(1);
});
```

Create `app/src/auth/useAuthStatus.test.tsx`:

```tsx
import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { commands, events, mockCommands } from "../bindings";
import { useAuthStatus } from "./useAuthStatus";

test("subscribes before asking for the current state", async () => {
  const order: string[] = [];
  mockCommands({
    authStatus: () => {
      order.push("authStatus");
      return "signed_out";
    },
  });
  const listen = events.authChanged.listen;
  vi.spyOn(events.authChanged, "listen").mockImplementation(async (callback) => {
    const stop = await listen(callback);
    order.push("listening");
    return stop;
  });
  const { result } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(result.current.status).toBe("signed_out"));
  expect(order).toEqual(["listening", "authStatus"]);
});

test("an event before the reply wins", async () => {
  let reply: ((status: "signed_out") => void) | undefined;
  mockCommands({ authStatus: () => new Promise((resolve) => (reply = resolve)) });
  const { result } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(reply).toBeDefined());
  await act(() => events.authChanged.emit({ status: "signed_in" }));
  await act(async () => reply?.("signed_out"));
  expect(result.current.status).toBe("signed_in");
});

test("unmounting stops listening", async () => {
  const stop = vi.fn();
  vi.spyOn(events.authChanged, "listen").mockResolvedValue(stop);
  vi.spyOn(commands, "authStatus").mockResolvedValue("signed_out");
  const { result, unmount } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(result.current.status).toBe("signed_out"));
  unmount();
  expect(stop).toHaveBeenCalledTimes(1);
});

test("unmounting before the subscription resolves still unsubscribes", async () => {
  const stop = vi.fn();
  let resolveListen: (stop: () => void) => void = () => {};
  vi.spyOn(events.authChanged, "listen").mockReturnValue(
    new Promise((resolve) => (resolveListen = resolve)),
  );
  const authStatus = vi.spyOn(commands, "authStatus").mockResolvedValue("signed_out");
  const { unmount } = renderHook(() => useAuthStatus());
  unmount();
  await act(async () => resolveListen(stop));
  expect(stop).toHaveBeenCalledTimes(1);
  expect(authStatus).not.toHaveBeenCalled();
});

test("a failed read reports the error until retried", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => {
      calls += 1;
      if (calls === 1) throw { kind: "internal", message: "store unavailable" };
      return "signed_in";
    },
  });
  const { result } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(result.current.error).toBe("store unavailable"));
  expect(result.current.status).toBe(null);
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.status).toBe("signed_in"));
  expect(result.current.error).toBe(null);
});
```

Replace `app/src/auth/SignInScreen.test.tsx` with:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { SignInScreen } from "./SignInScreen";

test("opens the sign-in window", async () => {
  const signIn = vi.fn(() => null);
  mockCommands({ signIn });
  render(<SignInScreen expired={false} />);

  expect(screen.getByText(/sign in with your google account/i)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));

  expect(signIn).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("explains an expired session in an alert", () => {
  render(<SignInScreen expired />);
  expect(screen.getByRole("alert")).toHaveTextContent(/your session expired/i);
  expect(screen.getByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("shows the error when the window cannot open", async () => {
  mockCommands({
    signIn: () => {
      throw { kind: "internal", message: "could not open the login window" };
    },
  });
  render(<SignInScreen expired={false} />);

  await userEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("could not open the login window");
});
```

Create `app/src/shell/shortcuts.test.ts`:

```typescript
import { expect, test } from "vitest";
import { shortcutFor } from "./shortcuts";

const body = document.body;

function key(key: string, init: Partial<Parameters<typeof shortcutFor>[0]> = {}) {
  return {
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    target: body,
    ...init,
  };
}

function element(html: string): Element {
  const wrapper = document.createElement("div");
  wrapper.innerHTML = html;
  return wrapper.firstElementChild as Element;
}

test("maps macOS shortcuts", () => {
  expect(shortcutFor(key(" "), true)).toBe("togglePlayback");
  expect(shortcutFor(key("ArrowRight", { metaKey: true }), true)).toBe("nextTrack");
  expect(shortcutFor(key("ArrowLeft", { metaKey: true }), true)).toBe("previousTrack");
  expect(shortcutFor(key("1", { metaKey: true }), true)).toBe("goHome");
  expect(shortcutFor(key("f", { metaKey: true }), true)).toBe("goSearch");
  expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), true)).toBe(null);
});

test("uses Ctrl elsewhere", () => {
  expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), false)).toBe("nextTrack");
  expect(shortcutFor(key("f", { ctrlKey: true }), false)).toBe("goSearch");
  expect(shortcutFor(key("ArrowRight", { metaKey: true }), false)).toBe(null);
});

test("leaves system and unrelated shortcuts alone", () => {
  for (const name of ["q", "w", "h", "m", ",", "r", "s"]) {
    expect(shortcutFor(key(name, { metaKey: true }), true)).toBe(null);
  }
  expect(shortcutFor(key("f", { metaKey: true, shiftKey: true }), true)).toBe(null);
  expect(shortcutFor(key(" ", { shiftKey: true }), true)).toBe(null);
});

test("does not steal keys from controls", () => {
  expect(shortcutFor(key(" ", { target: element("<button>Play</button>") }), true)).toBe(null);
  expect(shortcutFor(key(" ", { target: element('<input type="search">') }), true)).toBe(null);
  expect(shortcutFor(key(" ", { target: element('<input type="range">') }), true)).toBe(null);
  expect(shortcutFor(key("ArrowLeft", { metaKey: true, target: element("<textarea></textarea>") }), true)).toBe(
    null,
  );
  // ⌘F still reaches Search from a text field.
  expect(shortcutFor(key("f", { metaKey: true, target: element("<input>") }), true)).toBe("goSearch");
});

test("ignores held, composing, and handled keys", () => {
  expect(shortcutFor(key(" ", { repeat: true }), true)).toBe(null);
  expect(shortcutFor(key(" ", { isComposing: true }), true)).toBe(null);
  expect(shortcutFor(key(" ", { defaultPrevented: true }), true)).toBe(null);
});
```

Create `app/src/home/HomeScreen.test.tsx`:

```tsx
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { home } from "../test/fixtures";
import { renderWithProviders } from "../test/render";
import { HomeScreen } from "./HomeScreen";

test("the first all-song shelf is a song grid and the rest are carousels", async () => {
  mockCommands({ getHome: () => home });
  const { container } = renderWithProviders(<HomeScreen />);

  const quickPicks = await screen.findByRole("region", { name: "Quick picks" });
  expect(quickPicks.querySelector(".song-grid")).not.toBeNull();
  expect(within(quickPicks).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
    "Play First song, Test artist",
    "Play Second song",
  ]);

  const mixed = screen.getByRole("region", { name: "Mixed for you" });
  expect(mixed.querySelector(".carousel")).not.toBeNull();
  expect(within(mixed).getByText("Album • 2024")).toBeInTheDocument();
  // Only songs are interactive until album, artist, and playlist pages exist (M5).
  expect(within(mixed).getByRole("button", { name: "Play Mixed song" })).toBeInTheDocument();
  expect(within(mixed).queryByRole("button", { name: /Test album|Round artist/ })).not.toBeInTheDocument();
  expect(within(mixed).getByRole("button", { name: "Next in Mixed for you" })).toBeInTheDocument();

  const images = [...container.querySelectorAll("img")];
  expect(images.map((img) => img.getAttribute("src"))).toEqual([
    "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
    "https://lh3.googleusercontent.com/test-album=w226-h226",
    "https://yt3.ggpht.com/test-artist=s226",
  ]);
  expect(images[2]).toHaveClass("artwork-round");
  expect(images[1]).toHaveClass("artwork-square");
});

test("carousel buttons page by the visible width", async () => {
  const scrollBy = vi.fn();
  HTMLElement.prototype.scrollBy = scrollBy;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(600);
  try {
    mockCommands({ getHome: () => home });
    renderWithProviders(<HomeScreen />);
    await userEvent.click(await screen.findByRole("button", { name: "Next in Mixed for you" }));
    await userEvent.click(screen.getByRole("button", { name: "Previous in Mixed for you" }));
    expect(scrollBy.mock.calls).toEqual([
      [{ left: 600, behavior: "smooth" }],
      [{ left: -600, behavior: "smooth" }],
    ]);
  } finally {
    // @ts-expect-error -- jsdom has no scrollBy; remove the stub again.
    delete HTMLElement.prototype.scrollBy;
  }
});

test("clicking a song plays its shelf's songs from that song", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({ getHome: () => home, playQueue });
  renderWithProviders(<HomeScreen />);

  await userEvent.click(await screen.findByRole("button", { name: "Play Second song" }));
  expect(playQueue).toHaveBeenLastCalledWith({
    songs: [
      {
        video_id: "test-vid-01",
        title: "First song",
        artists: [{ name: "Test artist", browse_id: null }],
        thumbnail_url: "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
      },
      { video_id: "test-vid-02", title: "Second song", artists: [], thumbnail_url: null },
    ],
    startIndex: 1,
  });

  await userEvent.click(screen.getByRole("button", { name: "Play Mixed song" }));
  expect(playQueue).toHaveBeenLastCalledWith({
    songs: [{ video_id: "test-vid-03", title: "Mixed song", artists: [], thumbnail_url: null }],
    startIndex: 0,
  });
});

test("a failed play shows one notice", async () => {
  mockCommands({
    getHome: () => home,
    playQueue: () => {
      throw { kind: "not_signed_in", message: "Sign in to continue." };
    },
  });
  renderWithProviders(<HomeScreen />);
  await userEvent.click(await screen.findByRole("button", { name: "Play Second song" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Sign in to continue.");
  expect(screen.getAllByRole("alert")).toHaveLength(1);
});

test("load errors offer a retry", async () => {
  let calls = 0;
  mockCommands({
    getHome: () => {
      calls += 1;
      if (calls === 1) throw { kind: "network", message: "network error" };
      return home;
    },
  });
  renderWithProviders(<HomeScreen />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network error");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("region", { name: "Quick picks" })).toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("refresh refetches the feed", async () => {
  const getHome = vi.fn(() => home);
  mockCommands({ getHome });
  renderWithProviders(<HomeScreen />);
  await screen.findByRole("region", { name: "Quick picks" });
  await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await vi.waitFor(() => expect(getHome).toHaveBeenCalledTimes(2));
});

test("an empty feed says so", async () => {
  mockCommands({ getHome: () => ({ sections: [], continuation: null }) });
  renderWithProviders(<HomeScreen />);
  expect(await screen.findByText("Nothing to show yet.")).toBeInTheDocument();
});
```

Replace `app/src/player/PlayerBar.test.tsx` with:

```tsx
import { act, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "../bindings";
import { renderWithProviders } from "../test/render";
import { formatTime } from "./format";
import { PlayerBar } from "./PlayerBar";
import { idleSnapshot, playingState, testSong } from "./testing";

function renderPlaying(overrides: Parameters<typeof playingState>[0] = {}) {
  const handlers = {
    playerSnapshot: () => ({
      state: playingState(overrides),
      queue: { songs: [testSong, testSong], current: 0 },
    }),
    togglePlayback: vi.fn(() => null),
    nextTrack: vi.fn(() => null),
    previousTrack: vi.fn(() => null),
    seekTo: vi.fn(() => null),
    setRepeat: vi.fn(() => null),
    setShuffle: vi.fn(() => null),
  };
  mockCommands(handlers);
  renderWithProviders(<PlayerBar />);
  return handlers;
}

test("formats times as m:ss", () => {
  expect(formatTime(0)).toBe("0:00");
  expect(formatTime(65.9)).toBe("1:05");
  expect(formatTime(3600)).toBe("60:00");
  expect(formatTime(null)).toBe("0:00");
  expect(formatTime(-3)).toBe("0:00");
});

test("stays hidden while nothing is queued", async () => {
  const playerSnapshot = vi.fn(() => idleSnapshot);
  mockCommands({ playerSnapshot });
  renderWithProviders(<PlayerBar />);
  await vi.waitFor(() => expect(playerSnapshot).toHaveBeenCalled());
  expect(screen.queryByRole("contentinfo", { name: "Player" })).not.toBeInTheDocument();
});

test("shows the track from player://state", async () => {
  mockCommands({ playerSnapshot: () => idleSnapshot });
  renderWithProviders(<PlayerBar />);
  await act(() => events.playerState.emit({ state: playingState() }));
  expect(await screen.findByText("Test song")).toBeInTheDocument();
  expect(screen.getByText("Test artist")).toBeInTheDocument();
  expect(screen.getByText("1:05")).toBeInTheDocument();
  expect(screen.getByText("3:20")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
});

test("follows player://queue for the position in the queue", async () => {
  renderPlaying();
  expect(await screen.findByText("1 of 2")).toBeInTheDocument();
  await act(() =>
    events.playerQueue.emit({ queue: { songs: [testSong, testSong, testSong], current: 2 } }),
  );
  expect(await screen.findByText("3 of 3")).toBeInTheDocument();
});

test("transport buttons call the player commands", async () => {
  const handlers = renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: "Pause" }));
  await userEvent.click(screen.getByRole("button", { name: "Next" }));
  await userEvent.click(screen.getByRole("button", { name: "Previous" }));
  expect(handlers.togglePlayback).toHaveBeenCalledTimes(1);
  expect(handlers.nextTrack).toHaveBeenCalledTimes(1);
  expect(handlers.previousTrack).toHaveBeenCalledTimes(1);
});

test("next is disabled at the end of the queue", async () => {
  renderPlaying({ has_next: false, status: "paused" });
  expect(await screen.findByRole("button", { name: "Next" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
});

test("repeat cycles off, all, one and shuffle toggles", async () => {
  const handlers = renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: "Repeat off" }));
  expect(handlers.setRepeat).toHaveBeenLastCalledWith({ mode: "all" });
  await act(() => events.playerState.emit({ state: playingState({ repeat: "one", shuffle: true }) }));
  await userEvent.click(await screen.findByRole("button", { name: "Repeat one" }));
  expect(handlers.setRepeat).toHaveBeenLastCalledWith({ mode: "off" });
  const shuffle = screen.getByRole("button", { name: "Shuffle" });
  expect(shuffle).toHaveAttribute("aria-pressed", "true");
  await userEvent.click(shuffle);
  expect(handlers.setShuffle).toHaveBeenLastCalledWith({ enabled: false });
});

test("seeking commits when the slider is released", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  expect(handlers.seekTo).not.toHaveBeenCalled();
  expect(screen.getByText("2:00")).toBeInTheDocument();
  fireEvent.pointerUp(slider);
  expect(handlers.seekTo).toHaveBeenCalledWith({ position: 120 });
});

test("an ad replaces the timeline", async () => {
  renderPlaying({ is_ad: true });
  expect(await screen.findByText("Ad")).toBeInTheDocument();
  expect(screen.queryByRole("slider", { name: "Seek" })).not.toBeInTheDocument();
});

test("command errors are shown", async () => {
  mockCommands({
    playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }),
    nextTrack: () => {
      throw { kind: "internal", message: "player window failed" };
    },
  });
  renderWithProviders(<PlayerBar />);
  await userEvent.click(await screen.findByRole("button", { name: "Next" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("player window failed");
});

test("an event before the snapshot reply wins", async () => {
  let reply: ((snapshot: typeof idleSnapshot) => void) | undefined;
  mockCommands({ playerSnapshot: () => new Promise((resolve) => (reply = resolve)) });
  renderWithProviders(<PlayerBar />);
  await vi.waitFor(() => expect(reply).toBeDefined());
  await act(() => events.playerState.emit({ state: playingState() }));
  await act(async () => reply?.(idleSnapshot));
  expect(screen.getByText("Test song")).toBeInTheDocument();
});

test("a failed snapshot is reported", async () => {
  mockCommands({
    playerSnapshot: () => {
      throw { kind: "internal", message: "player unavailable" };
    },
  });
  renderWithProviders(<PlayerBar />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Could not read the player state: player unavailable",
  );
});

test("keyboard seeking commits on key release", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "90" } });
  fireEvent.keyUp(slider, { key: "ArrowRight" });
  expect(handlers.seekTo).toHaveBeenCalledWith({ position: 90 });
});

test("a cancelled drag or blur drops the drag position without seeking", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  fireEvent.pointerCancel(slider);
  expect(screen.getByText("1:05")).toBeInTheDocument();
  fireEvent.change(slider, { target: { value: "150" } });
  fireEvent.blur(slider);
  expect(screen.getByText("1:05")).toBeInTheDocument();
  fireEvent.pointerUp(slider);
  expect(handlers.seekTo).not.toHaveBeenCalled();
});

test("a seek does not snap back to stale progress", async () => {
  renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  fireEvent.pointerUp(slider);
  // Progress sampled before the page applied the seek.
  await act(() => events.playerState.emit({ state: playingState({ position: 66 }) }));
  expect(screen.getByText("2:00")).toBeInTheDocument();
  // The page caught up.
  await act(() => events.playerState.emit({ state: playingState({ position: 121 }) }));
  expect(screen.getByText("2:01")).toBeInTheDocument();
});

test("a seek override ends after the settle time", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    renderPlaying();
    const slider = await screen.findByRole("slider", { name: "Seek" });
    fireEvent.change(slider, { target: { value: "120" } });
    fireEvent.pointerUp(slider);
    await act(() => events.playerState.emit({ state: playingState({ position: 66 }) }));
    expect(screen.getByText("2:00")).toBeInTheDocument();
    await act(async () => vi.advanceTimersByTime(2100));
    expect(screen.getByText("1:06")).toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});

test("unmounting stops listening to player events", async () => {
  const stopState = vi.fn();
  const stopQueue = vi.fn();
  vi.spyOn(events.playerState, "listen").mockResolvedValue(stopState);
  vi.spyOn(events.playerQueue, "listen").mockResolvedValue(stopQueue);
  const playerSnapshot = vi.fn(() => idleSnapshot);
  mockCommands({ playerSnapshot });
  const { unmount } = renderWithProviders(<PlayerBar />);
  await vi.waitFor(() => expect(playerSnapshot).toHaveBeenCalled());
  unmount();
  await vi.waitFor(() => expect(stopState).toHaveBeenCalledTimes(1));
  expect(stopQueue).toHaveBeenCalledTimes(1);
});

test("the queue button shows the queue with the current song marked", async () => {
  renderPlaying();
  const button = await screen.findByRole("button", { name: /Queue/ });
  expect(button).toHaveAttribute("aria-expanded", "false");
  await userEvent.click(button);
  expect(button).toHaveAttribute("aria-expanded", "true");
  const panel = screen.getByRole("region", { name: "Queue" });
  const items = within(panel).getAllByRole("listitem");
  expect(items).toHaveLength(2);
  expect(items[0]).toHaveAttribute("aria-current", "true");
  expect(items[1]).not.toHaveAttribute("aria-current");
  await userEvent.click(button);
  expect(screen.queryByRole("region", { name: "Queue" })).not.toBeInTheDocument();
});

test("publishes its height for the scrolling content", async () => {
  const observers: ResizeObserverCallback[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: ResizeObserverCallback) {
        observers.push(callback);
      }
      observe() {}
      disconnect() {}
    },
  );
  try {
    mockCommands({ playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }) });
    const { container, unmount } = renderWithProviders(
      <div className="content-column">
        <PlayerBar />
      </div>,
    );
    await screen.findByRole("contentinfo", { name: "Player" });
    const column = container.querySelector(".content-column") as HTMLElement;
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(72);
    act(() => observers.forEach((callback) => callback([], {} as ResizeObserver)));
    expect(column.style.getPropertyValue("--player-space")).toBe("72px");
    unmount();
    expect(column.style.getPropertyValue("--player-space")).toBe("");
  } finally {
    vi.unstubAllGlobals();
  }
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/`): `pnpm vitest run`
Expected: FAIL — `App` has no `queryClient` prop, `./shortcuts` does not exist, the sign-in screen has no alert, and the new HomeScreen and PlayerBar tests fail on the missing grid, carousel, queue, notice, and seek behaviour.

- [ ] **Step 3: Implement the auth gate**

Replace `app/src/auth/useAuthStatus.ts` with:

```typescript
import { useCallback, useEffect, useState } from "react";
import { commands, events, type AuthStatus } from "../bindings";
import { errorMessage } from "../errors";

export type AuthView = {
  /** `null` until known, then kept current by `auth://changed`. */
  status: AuthStatus | null;
  /** Why the state could not be read; shown only while `status` is `null`. */
  error: string | null;
  retry: () => void;
};

/**
 * Subscribes to `auth://changed` before asking for the current state, so a
 * change between the reply and the subscription cannot be lost. An event that
 * arrives before the reply wins.
 */
export function useAuthStatus(): AuthView {
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    events.authChanged
      .listen((event) => {
        if (!active) return;
        setError(null);
        setStatus(event.payload.status);
      })
      .then((unlisten) => {
        if (!active) {
          unlisten();
          return;
        }
        stop = unlisten;
        return commands.authStatus().then((current) => {
          if (active) setStatus((previous) => previous ?? current);
        });
      })
      .catch((cause: unknown) => {
        if (active) setError(errorMessage(cause));
      });
    return () => {
      active = false;
      stop?.();
    };
  }, [attempt]);

  const retry = useCallback(() => {
    setError(null);
    setAttempt((count) => count + 1);
  }, []);

  return { status, error, retry };
}
```

Replace `app/src/App.tsx` with:

```tsx
import { QueryClientProvider, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { SignInScreen } from "./auth/SignInScreen";
import { useAuthStatus } from "./auth/useAuthStatus";
import { Shell } from "./shell/Shell";

export function App({ queryClient }: { queryClient: QueryClient }) {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthGate />
    </QueryClientProvider>
  );
}

function AuthGate() {
  const { status, error, retry } = useAuthStatus();
  const queryClient = useQueryClient();
  const signedIn = status === "signed_in";

  // Cached feeds belong to the account that fetched them.
  useEffect(() => {
    if (!signedIn) queryClient.clear();
  }, [signedIn, queryClient]);

  if (status === null) {
    return (
      <main className="center">
        {error === null ? (
          <p>Loading…</p>
        ) : (
          <div role="alert">
            <p>Could not read the sign-in state: {error}</p>
            <button type="button" onClick={retry}>
              Try again
            </button>
          </div>
        )}
      </main>
    );
  }
  if (signedIn) return <Shell />;
  return <SignInScreen expired={status === "expired"} />;
}
```

Replace `app/src/main.tsx` with:

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { commands } from "./bindings";
import { createQueryClient } from "./query";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

// The sidebar is see-through only when Rust applied native vibrancy (macOS)
// or Mica (Windows 11); otherwise it stays a solid color. Ask before the
// first paint so the sidebar does not change after it appears.
const vibrancy = await commands.windowVibrancy().catch(() => false);
document.documentElement.dataset.vibrancy = vibrancy ? "native" : "none";

createRoot(root).render(
  <StrictMode>
    <App queryClient={createQueryClient()} />
  </StrictMode>,
);
```

Replace `app/src/auth/SignInScreen.tsx` with:

```tsx
import { useState } from "react";
import { commands } from "../bindings";
import { errorMessage } from "../errors";

type Props = {
  /** The previous session was rejected by YouTube Music (HTTP 401/403). */
  expired: boolean;
};

export function SignInScreen({ expired }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  async function signIn() {
    setError(null);
    setOpening(true);
    try {
      await commands.signIn();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setOpening(false);
    }
  }

  return (
    <main className="center sign-in">
      <h1>Kaset</h1>
      {expired ? (
        <div className="alert" role="alert">
          Your session expired. Sign in again to continue.
        </div>
      ) : (
        <p className="hint">Sign in with your Google account to use YouTube Music.</p>
      )}
      <button type="button" className="primary" onClick={signIn} disabled={opening}>
        Sign in with Google
      </button>
      {error && <p role="alert">{error}</p>}
    </main>
  );
}
```

- [ ] **Step 4: Implement the shell and shortcuts**

Create `app/src/shell/shortcuts.ts`:

```typescript
export type Shortcut = "togglePlayback" | "nextTrack" | "previousTrack" | "goHome" | "goSearch";

type KeyInput = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "repeat" | "isComposing" | "defaultPrevented"
> & { target: EventTarget | null };

/** Controls where keys already mean something (typing, activating, sliding). */
const INTERACTIVE =
  "input, textarea, select, button, a[href], [contenteditable=''], [contenteditable='true'], [role='slider'], [role='option']";
const EDITABLE = "input, textarea, select, [contenteditable=''], [contenteditable='true']";

function matches(target: EventTarget | null, selector: string): boolean {
  return target instanceof Element && target.closest(selector) !== null;
}

export function isMac(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.userAgent);
}

/**
 * Maps a key press to an app shortcut, following macOS and Apple Music:
 * Space play/pause, ⌘→ next, ⌘← previous, ⌘1 Home, ⌘F Search (Ctrl instead
 * of ⌘ elsewhere). ⌘Q, ⌘W, ⌘H, ⌘M, and ⌘, are never taken.
 */
export function shortcutFor(event: KeyInput, mac: boolean = isMac()): Shortcut | null {
  if (event.defaultPrevented || event.isComposing) return null;
  const anyModifier = event.metaKey || event.ctrlKey || event.altKey || event.shiftKey;
  if (event.key === " " && !anyModifier) {
    return event.repeat || matches(event.target, INTERACTIVE) ? null : "togglePlayback";
  }
  const primary = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!primary || event.altKey || event.shiftKey) return null;
  switch (event.key) {
    case "ArrowRight":
      return matches(event.target, EDITABLE) ? null : "nextTrack";
    case "ArrowLeft":
      return matches(event.target, EDITABLE) ? null : "previousTrack";
    case "1":
      return "goHome";
    case "f":
    case "F":
      return "goSearch";
    default:
      return null;
  }
}
```

Create `app/src/shell/useShortcuts.ts`:

```typescript
import { useEffect } from "react";
import { shortcutFor, type Shortcut } from "./shortcuts";

/** Runs `actions[shortcut]` for app shortcuts pressed anywhere in the window. */
export function useShortcuts(actions: Record<Shortcut, () => void>) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const shortcut = shortcutFor(event);
      if (shortcut === null) return;
      event.preventDefault();
      actions[shortcut]();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [actions]);
}
```

Create `app/src/shell/Sidebar.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { Artwork } from "../music/Artwork";
import { useNotice } from "../notices/NoticeProvider";
import { ACCOUNT_STALE_MS, queryKeys } from "../query";

export type Route = "home" | "search";

const ROUTES: { route: Route; label: string }[] = [
  { route: "search", label: "Search" },
  { route: "home", label: "Home" },
];

type Props = {
  route: Route;
  onNavigate: (route: Route) => void;
};

export function Sidebar({ route, onNavigate }: Props) {
  const { report, dismiss } = useNotice();
  // Display only; a failure falls back to a generic label instead of an alert.
  const account = useQuery({
    queryKey: queryKeys.account,
    queryFn: () => commands.getAccount(),
    staleTime: ACCOUNT_STALE_MS,
  });

  async function signOut() {
    dismiss();
    try {
      await commands.signOut();
    } catch (cause) {
      report(`Could not sign out: ${errorMessage(cause)}`);
    }
  }

  const name = account.data?.name ?? "Signed in";
  return (
    <nav className="sidebar" aria-label="Main">
      <p className="app-name">Kaset</p>
      <ul className="nav">
        {ROUTES.map((item) => (
          <li key={item.route}>
            <button
              type="button"
              aria-current={route === item.route ? "page" : undefined}
              onClick={() => onNavigate(item.route)}
            >
              {item.label}
            </button>
          </li>
        ))}
      </ul>
      <section className="account" aria-label="Account">
        <Artwork url={account.data?.photo_url ?? null} size={32} shape="round" />
        <div className="item-text">
          <span className="item-title">{name}</span>
          {account.data?.handle && <span className="item-subtitle">{account.data.handle}</span>}
        </div>
        <button type="button" className="ghost" onClick={() => void signOut()}>
          Sign out
        </button>
      </section>
    </nav>
  );
}
```

Create `app/src/shell/Shell.tsx`:

```tsx
import { useMemo, useState } from "react";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { HomeScreen } from "../home/HomeScreen";
import { NoticeBanner, NoticeProvider, useNotice } from "../notices/NoticeProvider";
import { PlayerBar } from "../player/PlayerBar";
import { SearchScreen } from "../search/SearchScreen";
import type { Shortcut } from "./shortcuts";
import { Sidebar, type Route } from "./Sidebar";
import { useShortcuts } from "./useShortcuts";

/** The signed-in app: sidebar, the current screen, notices, and the floating player bar. */
export function Shell() {
  return (
    <NoticeProvider>
      <ShellLayout />
    </NoticeProvider>
  );
}

function ShellLayout() {
  const [route, setRoute] = useState<Route>("home");
  // Kept here so leaving Search and coming back shows the same results.
  const [query, setQuery] = useState("");
  // Bumped by ⌘F so the search field takes focus even when Search is open.
  const [searchFocus, setSearchFocus] = useState(0);
  const { report, dismiss } = useNotice();

  const actions = useMemo<Record<Shortcut, () => void>>(() => {
    const run = (command: () => Promise<unknown>) => {
      dismiss();
      command().catch((cause: unknown) => report(errorMessage(cause)));
    };
    return {
      togglePlayback: () => run(() => commands.togglePlayback()),
      nextTrack: () => run(() => commands.nextTrack()),
      previousTrack: () => run(() => commands.previousTrack()),
      goHome: () => setRoute("home"),
      goSearch: () => {
        setRoute("search");
        setSearchFocus((count) => count + 1);
      },
    };
  }, [report, dismiss]);
  useShortcuts(actions);

  return (
    <div className="shell">
      <Sidebar route={route} onNavigate={setRoute} />
      <div className="content-column">
        <main className="content">
          {route === "home" ? (
            <HomeScreen />
          ) : (
            <SearchScreen query={query} onSubmit={setQuery} focusRequest={searchFocus} />
          )}
        </main>
        <NoticeBanner />
        <PlayerBar />
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Rebuild the home screen and the player bar**

Replace `app/src/home/HomeScreen.tsx` with:

```tsx
import { useQuery } from "@tanstack/react-query";
import { useId, useRef } from "react";
import { commands, type HomeSection, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { ItemCard } from "../music/ItemCard";
import { isPlayable, itemKey, shapeOf, subtitleOf, toSong } from "../music/items";
import { usePlayQueue } from "../player/usePlayQueue";
import { HOME_STALE_MS, queryKeys } from "../query";

type PlayFn = (songs: Song[], index: number) => Promise<void>;

export function HomeScreen() {
  const home = useQuery({
    queryKey: queryKeys.home,
    queryFn: () => commands.getHome(),
    staleTime: HOME_STALE_MS,
  });
  const play = usePlayQueue();
  const sections = home.data?.sections ?? [];
  // The first all-song shelf (YouTube Music's "Quick picks") is a song grid.
  const gridIndex = sections.findIndex((section) => section.items.every(isPlayable));

  return (
    <>
      <header className="content-header">
        <h1>Home</h1>
        <button type="button" className="ghost" onClick={() => void home.refetch()} disabled={home.isFetching}>
          Refresh
        </button>
      </header>
      {home.isPending && <p className="hint">Loading…</p>}
      {home.isError && (
        // An auth_expired error also arrives as auth://changed, which replaces the shell.
        <div role="alert" className="screen-error">
          <p>{errorMessage(home.error)}</p>
          <button type="button" onClick={() => void home.refetch()}>
            Retry
          </button>
        </div>
      )}
      {home.data && sections.length === 0 && <p className="hint">Nothing to show yet.</p>}
      {sections.map((section, index) =>
        index === gridIndex ? (
          <SongGrid key={`${index}-${section.title}`} section={section} onPlay={play} />
        ) : (
          <Carousel key={`${index}-${section.title}`} section={section} onPlay={play} />
        ),
      )}
    </>
  );
}

/** Songs of a shelf, and for each item its index among them (-1 for non-songs). */
function playOrder(section: HomeSection): { songs: Song[]; starts: number[] } {
  const songs = section.items.filter(isPlayable).map(toSong);
  let next = 0;
  const starts = section.items.map((item) => (isPlayable(item) ? next++ : -1));
  return { songs, starts };
}

type ShelfProps = { section: HomeSection; onPlay: PlayFn };

/** Quick picks: a three-column grid of song rows. */
function SongGrid({ section, onPlay }: ShelfProps) {
  const headingId = useId();
  const { songs, starts } = playOrder(section);
  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header">
        <h2 id={headingId}>{section.title}</h2>
      </div>
      <ul className="song-grid">
        {section.items.map((item, index) => (
          <li key={`${index}-${itemKey(item)}`}>
            <ItemCard
              layout="row"
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onPlay={() => void onPlay(songs, starts[index])}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** A shelf as a six-column carousel with previous/next buttons. */
function Carousel({ section, onPlay }: ShelfProps) {
  const headingId = useId();
  const list = useRef<HTMLUListElement>(null);
  const { songs, starts } = playOrder(section);

  function page(direction: 1 | -1) {
    const element = list.current;
    element?.scrollBy?.({ left: direction * element.clientWidth, behavior: "smooth" });
  }

  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header">
        <h2 id={headingId}>{section.title}</h2>
        <div className="shelf-nav">
          <button type="button" className="icon-button" aria-label={`Previous in ${section.title}`} onClick={() => page(-1)}>
            ‹
          </button>
          <button type="button" className="icon-button" aria-label={`Next in ${section.title}`} onClick={() => page(1)}>
            ›
          </button>
        </div>
      </div>
      <ul ref={list} className="carousel">
        {section.items.map((item, index) => (
          <li key={`${index}-${itemKey(item)}`}>
            <ItemCard
              layout="card"
              shape={shapeOf(item)}
              title={item.title}
              subtitle={subtitleOf(item)}
              thumbnailUrl={item.thumbnail_url}
              onPlay={starts[index] >= 0 ? () => void onPlay(songs, starts[index]) : undefined}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}
```

Replace `app/src/player/usePlayer.ts` with:

```typescript
import { useEffect, useState } from "react";
import { commands, events, type PlayerState, type QueueState } from "../bindings";
import { errorMessage } from "../errors";

export type PlayerView = {
  state: PlayerState | null;
  queue: QueueState | null;
};

/**
 * The player state and queue: `null` until known, then kept current by
 * `player://state` and `player://queue`. Events that arrive before the
 * initial `playerSnapshot` reply win. `onError` hears a failed snapshot;
 * pass a stable function.
 */
export function usePlayer(onError: (message: string) => void): PlayerView {
  const [state, setState] = useState<PlayerState | null>(null);
  const [queue, setQueue] = useState<QueueState | null>(null);

  useEffect(() => {
    let active = true;
    const stopState = events.playerState.listen((event) => {
      if (active) setState(event.payload.state);
    });
    const stopQueue = events.playerQueue.listen((event) => {
      if (active) setQueue(event.payload.queue);
    });
    commands.playerSnapshot().then(
      (snapshot) => {
        if (!active) return;
        setState((previous) => previous ?? snapshot.state);
        setQueue((previous) => previous ?? snapshot.queue);
      },
      (cause: unknown) => {
        if (active) onError(`Could not read the player state: ${errorMessage(cause)}`);
      },
    );
    return () => {
      active = false;
      void stopState.then((stop) => stop());
      void stopQueue.then((stop) => stop());
    };
  }, [onError]);

  return { state, queue };
}
```

Replace `app/src/player/PlayerBar.tsx` with:

```tsx
import { useEffect, useId, useRef, useState } from "react";
import { commands, type RepeatMode } from "../bindings";
import { errorMessage } from "../errors";
import { Artwork } from "../music/Artwork";
import { artistNames } from "../music/items";
import { useNotice } from "../notices/NoticeProvider";
import { formatTime } from "./format";
import { usePlayer } from "./usePlayer";

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: "all", all: "one", one: "off" };
const REPEAT_LABEL: Record<RepeatMode, string> = {
  off: "Repeat off",
  all: "Repeat all",
  one: "Repeat one",
};

/** After a seek, keep showing the target until the page reports a position this close… */
const SEEK_TOLERANCE_SECONDS = 1.5;
/** …or until this much time has passed. */
const SEEK_SETTLE_MS = 2000;

/**
 * The floating player bar: transport on the left, now playing with progress in
 * the middle, shuffle/repeat/queue on the right. Hidden while nothing is
 * queued. Publishes its height as `--player-space` on its parent so the
 * scrolling content can leave room below the last row.
 */
export function PlayerBar() {
  const { report, dismiss } = useNotice();
  const { state, queue } = usePlayer(report);
  // While the user drags the seek slider, show the drag position.
  const [dragPosition, setDragPosition] = useState<number | null>(null);
  // After a seek, progress sampled before the page applied it must not snap the slider back.
  const [pendingSeek, setPendingSeek] = useState<number | null>(null);
  const [queueOpen, setQueueOpen] = useState(false);
  const settleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const bar = useRef<HTMLElement>(null);
  const queueId = useId();
  const visible = state !== null && state.track !== null;

  useEffect(() => () => clearTimeout(settleTimer.current), []);

  useEffect(() => {
    const element = bar.current;
    const column = element?.parentElement;
    if (!element || !column || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() =>
      column.style.setProperty("--player-space", `${element.offsetHeight}px`),
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      column.style.removeProperty("--player-space");
    };
  }, [visible]);

  if (state === null || state.track === null) return null;

  function run(action: () => Promise<unknown>) {
    dismiss();
    action().catch((cause: unknown) => report(errorMessage(cause)));
  }

  function commitSeek() {
    if (dragPosition === null) return;
    const target = dragPosition;
    setDragPosition(null);
    setPendingSeek(target);
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => setPendingSeek(null), SEEK_SETTLE_MS);
    run(() => commands.seekTo(target));
  }

  const track = state.track;
  const artists = artistNames(track.artists);
  const playing = state.status === "playing" || state.status === "loading";
  const duration = state.duration ?? 0;
  const reported = state.position ?? 0;
  const settled = pendingSeek === null || Math.abs(reported - pendingSeek) <= SEEK_TOLERANCE_SECONDS;
  const position = dragPosition ?? (settled ? reported : pendingSeek);

  return (
    <footer ref={bar} className="player-bar" aria-label="Player">
      <div className="player-controls">
        <button type="button" className="icon-button" onClick={() => run(() => commands.previousTrack())}>
          Previous
        </button>
        <button type="button" className="play-button" onClick={() => run(() => commands.togglePlayback())}>
          {playing ? "Pause" : "Play"}
        </button>
        <button
          type="button"
          className="icon-button"
          disabled={!state.has_next}
          onClick={() => run(() => commands.nextTrack())}
        >
          Next
        </button>
      </div>
      <div className="player-now">
        <Artwork url={track.thumbnail_url} size={44} />
        <div className="player-meta">
          <div className="player-title">
            <strong className="item-title">{track.title}</strong>
            {artists && <span className="item-subtitle">{artists}</span>}
          </div>
          <div className="player-progress">
            {state.is_ad ? (
              <span className="player-ad">Ad</span>
            ) : (
              <>
                <span>{formatTime(position)}</span>
                <input
                  type="range"
                  aria-label="Seek"
                  aria-valuetext={`${formatTime(position)} of ${formatTime(duration)}`}
                  min={0}
                  max={duration}
                  step={1}
                  value={Math.min(position, duration)}
                  disabled={duration <= 0}
                  onChange={(event) => setDragPosition(Number(event.target.value))}
                  onPointerUp={commitSeek}
                  onKeyUp={commitSeek}
                  // A cancelled drag or focus loss must not leave the slider stuck on the drag value.
                  onPointerCancel={() => setDragPosition(null)}
                  onBlur={() => setDragPosition(null)}
                />
                <span>{formatTime(duration)}</span>
              </>
            )}
          </div>
        </div>
      </div>
      <div className="player-extras">
        <button
          type="button"
          className="icon-button"
          aria-pressed={state.shuffle}
          onClick={() => run(() => commands.setShuffle(!state.shuffle))}
        >
          Shuffle
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={() => run(() => commands.setRepeat(NEXT_REPEAT[state.repeat]))}
        >
          {REPEAT_LABEL[state.repeat]}
        </button>
        <button
          type="button"
          className="icon-button"
          aria-expanded={queueOpen}
          aria-controls={queueId}
          onClick={() => setQueueOpen((open) => !open)}
        >
          Queue
          {queue && queue.current !== null && (
            <span className="player-queue">
              {" "}
              {queue.current + 1} of {queue.songs.length}
            </span>
          )}
        </button>
      </div>
      {queueOpen && (
        <section id={queueId} className="queue-panel" aria-label="Queue">
          <ol>
            {(queue?.songs ?? []).map((song, index) => (
              <li key={`${index}-${song.video_id}`} aria-current={index === queue?.current ? "true" : undefined}>
                <span className="item-title">{song.title}</span>
                <span className="item-subtitle">{artistNames(song.artists)}</span>
              </li>
            ))}
          </ol>
        </section>
      )}
    </footer>
  );
}
```

- [ ] **Step 6: Replace the stylesheet**

Replace `app/src/styles.css` with:

```css
/* Dark theme from the approved M4 mockup. The window is transparent so the
   sidebar can show native vibrancy (macOS) or Mica (Windows 11); everything
   else paints an opaque background. */
:root {
  color-scheme: dark;
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  font-size: 14px;
  --accent: #d91c48;
  --accent-hover: #e8335d;
  --bg: #121214;
  --surface: #1c1c1f;
  --surface-hover: #26262a;
  --sidebar-solid: #18181b;
  --sidebar-glass: rgb(24 24 27 / 0.35);
  --glass: rgb(28 28 31 / 0.72);
  /* Popups sit over busy content, so they keep more of their own color. */
  --glass-strong: rgb(28 28 31 / 0.92);
  --line: rgb(255 255 255 / 0.08);
  --text: #f4f4f5;
  --muted: #a1a1aa;
  --blur: saturate(160%) blur(20px);
  --header-height: 56px;
  --inset: 24px;
  --radius: 12px;
}

html,
body,
#root {
  height: 100%;
  margin: 0;
}

body {
  background: var(--bg);
  color: var(--text);
}

/* With native vibrancy only the content column paints; the sidebar shows the effect. */
:root[data-vibrancy="native"] body {
  background: transparent;
}

button {
  font: inherit;
  color: inherit;
}

:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.hint {
  color: var(--muted);
}

/* Buttons */
.primary {
  padding: 8px 18px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border: none;
  border-radius: 999px;
  cursor: pointer;
}

.primary:hover {
  background: var(--accent-hover);
}

.primary:disabled {
  opacity: 0.6;
  cursor: default;
}

.ghost,
.icon-button {
  padding: 6px 12px;
  background: none;
  border: 1px solid var(--line);
  border-radius: 999px;
  cursor: pointer;
}

.ghost:hover,
.icon-button:hover {
  background: var(--surface-hover);
}

.icon-button[aria-pressed="true"] {
  color: var(--accent-hover);
  border-color: var(--accent);
}

.icon-button:disabled {
  opacity: 0.4;
  cursor: default;
}

/* Loading, auth error, and sign-in screens */
.center {
  box-sizing: border-box;
  min-height: 100%;
  max-width: 480px;
  margin: 0 auto;
  padding: 72px 24px;
  background: var(--bg);
}

:root[data-vibrancy="native"] .center {
  max-width: none;
  padding-inline: max(24px, calc((100% - 432px) / 2));
}

.sign-in h1 {
  font-size: 2rem;
}

.alert {
  margin: 16px 0;
  padding: 12px 16px;
  border: 1px solid var(--accent);
  border-radius: var(--radius);
  background: rgb(217 28 72 / 0.12);
}

/* Shell: 240 px sidebar and a content column that scrolls under a sticky
   header and the floating player bar. */
.shell {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  height: 100%;
}

.sidebar {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-height: 0;
  padding: 16px 12px;
  background: var(--sidebar-solid);
  border-right: 1px solid var(--line);
  overflow-y: auto;
}

:root[data-vibrancy="native"] .sidebar {
  background: var(--sidebar-glass);
}

.app-name {
  margin: 4px 12px 16px;
  font-size: 1.1rem;
  font-weight: 700;
}

.nav {
  display: grid;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.nav button {
  width: 100%;
  padding: 8px 14px;
  text-align: left;
  color: var(--muted);
  background: none;
  border: none;
  border-radius: 999px;
  cursor: pointer;
}

.nav button:hover {
  color: var(--text);
  background: var(--surface-hover);
}

.nav button[aria-current="page"] {
  font-weight: 600;
  color: #fff;
  background: var(--accent);
}

.account {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 8px 10px;
  margin-top: auto;
  padding: 12px;
  border-top: 1px solid var(--line);
}

.account .ghost {
  grid-column: 1 / -1;
}

.content-column {
  position: relative;
  min-width: 0;
  min-height: 0;
  background: var(--bg);
}

.content {
  box-sizing: border-box;
  height: 100%;
  padding: 0 32px calc(var(--player-space, 0px) + 2 * var(--inset));
  overflow-y: auto;
}

.content-header {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  height: var(--header-height);
  margin: 0 -32px 8px;
  padding: 0 32px;
  background: var(--glass);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border-bottom: 1px solid var(--line);
}

.content-header h1 {
  margin: 0;
  font-size: 1.25rem;
}

.notice,
.screen-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border: 1px solid var(--accent);
  border-radius: var(--radius);
  background: var(--surface);
}

.notice p,
.screen-error p {
  margin: 0;
}

.notice {
  position: absolute;
  z-index: 4;
  top: calc(var(--header-height) + 12px);
  right: var(--inset);
  left: var(--inset);
}

.screen-error {
  margin: 16px 0;
}

/* Home */
.shelf {
  margin-top: 24px;
}

.shelf-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}

.shelf-header h2,
.result-group h2,
.top-result h2 {
  margin: 0;
  font-size: 1.15rem;
}

.shelf-nav {
  display: flex;
  gap: 6px;
}

.song-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 4px 16px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.carousel {
  --columns: 6;
  --gap: 16px;
  display: grid;
  grid-auto-flow: column;
  grid-auto-columns: calc((100% - (var(--columns) - 1) * var(--gap)) / var(--columns));
  gap: var(--gap);
  margin: 0;
  padding: 0;
  list-style: none;
  overflow-x: auto;
  scroll-snap-type: x mandatory;
  scrollbar-width: none;
}

.carousel > li {
  scroll-snap-align: start;
}

.rows {
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
}

/* Items */
.item {
  display: flex;
  min-width: 0;
  padding: 0;
  text-align: left;
  background: none;
  border: none;
  border-radius: 8px;
}

button.item {
  cursor: pointer;
}

button.item:hover {
  background: var(--surface-hover);
}

.item-card {
  flex-direction: column;
  gap: 8px;
  width: 100%;
  padding-bottom: 6px;
}

.item-row {
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 6px;
}

.item-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.item-title,
.item-subtitle {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.item-subtitle {
  font-size: 0.9em;
  color: var(--muted);
}

.artwork {
  flex: none;
  object-fit: cover;
  background: var(--surface);
}

.artwork-square {
  border-radius: 6px;
}

.artwork-round {
  border-radius: 50%;
}

.artwork-fill {
  width: 100%;
  height: auto;
  aspect-ratio: 1;
}

.artwork-placeholder {
  display: block;
  background: var(--surface-hover);
}

/* Search */
.search-box {
  position: relative;
  flex: 1;
  max-width: 560px;
}

.search-box input {
  box-sizing: border-box;
  width: 100%;
  padding: 8px 14px;
  font: inherit;
  color: var(--text);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 999px;
}

.suggestions {
  position: absolute;
  z-index: 3;
  right: 0;
  left: 0;
  margin: 6px 0 0;
  padding: 6px;
  list-style: none;
  background: var(--glass-strong);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 12px 32px rgb(0 0 0 / 0.45);
}

.suggestions li {
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
}

.suggestions li:hover,
.suggestions li[aria-selected="true"] {
  background: var(--surface-hover);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 16px 0;
}

.chip {
  padding: 6px 14px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 999px;
  cursor: pointer;
}

.chip[aria-pressed="true"] {
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}

.top-result,
.result-group {
  margin-top: 24px;
}

.top-card {
  display: flex;
  align-items: center;
  gap: 20px;
  max-width: 560px;
  margin-top: 10px;
  padding: 20px;
  background: var(--surface);
  border-radius: var(--radius);
}

.top-text p {
  margin: 0 0 8px;
}

.top-title {
  font-size: 1.4rem;
  font-weight: 700;
}

/* Floating player bar, inset from the content edges, blurred over the content. */
.player-bar {
  position: absolute;
  z-index: 3;
  right: var(--inset);
  bottom: var(--inset);
  left: var(--inset);
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 20px;
  padding: 10px 16px;
  background: var(--glass);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-radius: 16px;
  box-shadow: 0 12px 40px rgb(0 0 0 / 0.5);
}

.player-controls,
.player-extras {
  display: flex;
  align-items: center;
  gap: 6px;
}

.play-button {
  min-width: 72px;
  padding: 8px 14px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border: none;
  border-radius: 999px;
  cursor: pointer;
}

.player-now {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.player-meta {
  flex: 1;
  min-width: 0;
}

.player-title {
  display: flex;
  gap: 8px;
  align-items: baseline;
  min-width: 0;
}

.player-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.85em;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.player-progress input {
  flex: 1;
  min-width: 80px;
  accent-color: var(--accent);
}

.player-queue {
  color: var(--muted);
}

.player-ad {
  padding: 0 6px;
  border-radius: 4px;
  background: #f5c518;
  color: #000;
  font-weight: 600;
}

.queue-panel {
  position: absolute;
  right: 0;
  bottom: calc(100% + 8px);
  width: 320px;
  max-height: 360px;
  overflow-y: auto;
  background: var(--glass-strong);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.queue-panel ol {
  margin: 0;
  padding: 6px;
  list-style: none;
}

.queue-panel li {
  display: flex;
  flex-direction: column;
  padding: 6px 10px;
  border-radius: 8px;
}

.queue-panel li[aria-current="true"] {
  background: rgb(217 28 72 / 0.2);
}
```

- [ ] **Step 7: Document the shortcuts**

Append to the end of `docs/keyboard-shortcuts.md`:

```markdown

## Tauri app (in progress)

The Tauri port (ADR-0042, ADR-0045) implements a subset so far. Off macOS, Ctrl
replaces ⌘. The app never takes ⌘Q, ⌘W, ⌘H, ⌘M, or ⌘,.

| Shortcut | Action |
| -------- | ------ |
| `Space`  | Play / Pause (not while a button, slider, or text field has focus) |
| `⌘→`     | Next track (not in text fields) |
| `⌘←`     | Previous track (not in text fields) |
| `⌘1`     | Go to Home |
| `⌘F`     | Go to Search and focus the search field |

In the search field, `↓`/`↑` move through suggestions, `Return` searches the
highlighted suggestion (or the typed text), and `Esc` closes the suggestions.
```

- [ ] **Step 8: Run the checks**

Run (from `app/`): `pnpm test && pnpm lint && pnpm typecheck && pnpm build`
Expected: `Test Files  11 passed (11)`, `Tests  67 passed (67)`; lint, typecheck, and the Vite build clean (`main.tsx` uses top-level `await`, which Vite 8 builds for the WebView targets).

- [ ] **Step 9: Commit and push**

```bash
git add app/src/main.tsx app/src/App.tsx app/src/App.test.tsx app/src/auth/ app/src/shell/ app/src/home/ app/src/player/usePlayer.ts app/src/player/PlayerBar.tsx app/src/player/PlayerBar.test.tsx app/src/styles.css app/src/test/render.tsx docs/keyboard-shortcuts.md
git commit -m "feat(app): add the dark UI shell with sidebar, home grid, floating player bar, and shortcuts"
git push origin main
gh run list --limit 4
```

Wait for the `App` run on this commit (`gh run watch <run-id> --exit-status`). Expected: green on macOS and Windows.

---

### Task 9: Real app icon

**Files:**
- Create: `app/scripts/render-icon.swift`, `app/src-tauri/app-icon.png` (generated, committed as the icon source)
- Modify (generated by `pnpm tauri icon`): `app/src-tauri/icons/{32x32.png,128x128.png,128x128@2x.png,icon.icns,icon.ico,icon.png,Square30x30Logo.png,Square44x44Logo.png,Square71x71Logo.png,Square89x89Logo.png,Square107x107Logo.png,Square142x142Logo.png,Square150x150Logo.png,Square284x284Logo.png,Square310x310Logo.png,StoreLogo.png}`

**Interfaces:**
- Consumes: `Sources/Kaset/Resources/AppIcon.icon/Assets/Image Layer.png` (read only); `tauri.conf.json` `bundle.icon` (unchanged: `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png`, `icons/icon.icns`, `icons/icon.ico`).
- Produces: `swift app/scripts/render-icon.swift <layer.png> <out.png>` writes a 1024×1024 PNG.

- [ ] **Step 1: Add the renderer**

Create `app/scripts/render-icon.swift`:

```swift
// Renders the Tauri icon source (1024x1024 PNG) from the Swift app's Icon Composer layer.
// Usage: swift render-icon.swift <Image Layer.png> <output.png>
import AppKit

let args = CommandLine.arguments
guard args.count == 3, let layer = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write(Data("usage: render-icon.swift <layer.png> <out.png>\n".utf8))
    exit(1)
}
let size = 1024
guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0
) else { exit(1) }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
// macOS icon grid: an 824-point body centred on the 1024 canvas.
let body = NSRect(x: 100, y: 100, width: 824, height: 824)
let shape = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)
NSGradient(
    starting: NSColor(white: 1.0, alpha: 1), ending: NSColor(white: 0.93, alpha: 1)
)?.draw(in: shape, angle: -90)
// The cassette layer, about two thirds of the canvas wide, slightly above centre.
let width = 672.0
let height = width * layer.size.height / layer.size.width
let rect = NSRect(x: (1024 - width) / 2, y: 1024 * 0.54 - height / 2, width: width, height: height)
layer.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
try png.write(to: URL(fileURLWithPath: args[2]))
```

- [ ] **Step 2: Render the source and regenerate the icons**

Run (from `app/`):

```bash
swift scripts/render-icon.swift "../Sources/Kaset/Resources/AppIcon.icon/Assets/Image Layer.png" src-tauri/app-icon.png
sips -g pixelWidth -g pixelHeight src-tauri/app-icon.png    # 1024 x 1024
pnpm tauri icon src-tauri/app-icon.png
rm -r src-tauri/icons/android src-tauri/icons/ios src-tauri/icons/64x64.png   # mobile/unused outputs
git status --short src-tauri/icons
```

Expected: the 16 files listed under **Files** show as modified and nothing else is added. Open `src-tauri/app-icon.png` in Preview: a pink cassette on a white rounded square (not the Tauri logo).

- [ ] **Step 3: Verify the bundle**

Run (from `app/`): `pnpm tauri build --bundles app`
Expected: `Kaset.app` is built; `plutil -p ../target/release/bundle/macos/Kaset.app/Contents/Info.plist | grep CFBundleIconFile` prints `icon.icns`.

- [ ] **Step 4: Commit and push**

```bash
git add app/scripts/render-icon.swift app/src-tauri/app-icon.png app/src-tauri/icons/
git commit -m "feat(app): use the Kaset icon instead of the Tauri placeholder"
git push origin main
gh run list --limit 4
```

Wait for the `App` run (`gh run watch <run-id> --exit-status`). Expected: green on macOS and Windows (the Windows NSIS build embeds `icon.ico`).

---

### Task 10: ADR-0045 and AGENTS.md

**Files:**
- Create: `docs/adr/0045-tauri-ui-shell.md`
- Modify: `AGENTS.md` (the `## Tauri port (in progress)` section only)

**Interfaces:** none (documentation).

- [ ] **Step 1: Write the ADR**

Create `docs/adr/0045-tauri-ui-shell.md`:

```markdown
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
```

- [ ] **Step 2: Update AGENTS.md**

In `AGENTS.md`, inside `## Tauri port (in progress)`, insert after the bullet that starts with `- OS media controls (\`souvlaki\`)`:

```markdown
- UI (ADR-0045): server data goes through TanStack Query (`app/src/query.ts`:
  `queryKeys`, stale times, retry policy); do not fetch in `useEffect`. Action
  errors go to the single notice area (`useNotice().report`), load errors stay
  in the screen with Retry. Navigation is state-based (`shell/Sidebar.tsx`
  `Route`); there is no router. Artwork goes through `music/Artwork.tsx`, which
  only loads https URLs on the hosts the CSP allows. The main window is
  transparent: screens paint `--bg`; only the sidebar is translucent, and only
  when `<html data-vibrancy="native">` (set from the `window_vibrancy`
  command). Every `backdrop-filter` needs the `-webkit-` prefix. New API commands use
  `signed_in_request` in `commands.rs`. Test screens with
  `test/render.tsx` (`renderWithProviders`, `renderApp`) and `mockCommands`.
- Search (`ytm-core::parsers::search`) is a port of the Swift parser verified
  with `cargo run -p ytm-explorer -- search "<query>" --parse` and a signed-in
  `KASET_COOKIE_FILE` outside the repository.
```

- [ ] **Step 3: Commit and push**

```bash
git add docs/adr/0045-tauri-ui-shell.md AGENTS.md
git commit -m "docs: record Tauri UI shell decisions (ADR-0045)"
git push origin main
```

---

## Human runtime checklist (after Task 10; not a Codex task)

Codex workers cannot sign in or watch the desktop. The human runs this with the YouTube Music Premium account and records results (statuses, renderer names, and host names only — never cookie values, names, or emails) in `docs/superpowers/spikes/2026-11-22-tauri-playback.md` under a new "M4 app" heading.

**Before the app:** complete Task 4 Step 8 (live `ytm-explorer search` / `suggest` / `account` / `browse FEmusic_home --parse home`).

**macOS** (from `app/`, `KASET_SESSION_STORE=file pnpm tauri dev`, signed in from M3 or sign in now):

1. The Dock and the app switcher show the pink cassette icon (also after `pnpm tauri build --bundles app`).
2. **Vibrancy:** the sidebar shows the desktop/wallpaper blurred through it (macOS Sidebar material) and dims when the window loses focus; the content column, header, and sign-in screen are opaque dark, with nothing of the desktop showing through anywhere else. The terminal logs `main window effects vibrancy=true`. Note whether the title bar looks right with the dark theme and transparent window, and whether resizing flickers.
3. **Layout:** sidebar 240 px with Search and Home (Home is the pink pill), the account block at the bottom shows your name and handle (note whether it is the expected account), and Sign out. The 56 px header stays at the top while Home scrolls, and shelves visibly blur under it.
4. Home: the first shelf (Quick picks) is a three-column grid of rows with 44 px thumbnails; the other shelves are six-card carousels with ‹ › buttons that page sideways; artist cards are round. Note any shelf on music.youtube.com's home that is missing here and whether any cards show placeholders instead of artwork. The Web Inspector console (right-click → Inspect Element) shows no `Content Security Policy` violations for images.
5. Click a Quick picks song → playback starts within a few seconds; the player bar floats 24 px above the bottom and in from the content edges, rounded and blurred over the content behind it, with controls left, artwork/title/progress middle, and Shuffle / Repeat / Queue right. Scroll to the very bottom of Home: the last shelf ends above the bar, not under it. Queue opens the list with the current song marked.
6. Drag the seek slider and release → audio jumps and the slider does not snap back. Start a drag and press Escape or click outside the window → the slider returns to the playing position.
7. Search: ⌘F (or the sidebar) opens Search with the header field focused. Type a query → a blurred suggestions dropdown appears after a short pause; ↓/↑ move, Return searches, a click searches. Results: the chips All / Songs / Albums / Artists / Playlists, a Top result card (Play appears only for a song/video/episode), then grouped results; chips switch instantly without a spinner. Clicking a song plays that group from it; albums/artists/playlists are visible and not clickable. Go Home and back → the previous query and results are still there.
8. Shortcuts with focus outside text fields: Space pauses/resumes, ⌘→ next, ⌘← previous, ⌘1 Home, ⌘F Search. In the search field, Space types a space and ⌘← moves the caret. ⌘Q quits, ⌘W closes the window, ⌘H hides, ⌘M minimizes (all unchanged).
9. Home → Refresh reloads the shelves. Turn Wi-Fi off and Refresh → an error with Retry appears in the screen; turn it back on and Retry recovers. With Wi-Fi off, click a song → one notice floats under the header (not two); Dismiss removes it.
10. Sign out → the sign-in screen (opaque, dark) appears and audio stops. Sign in with the same account → Home loads fresh (no stale shelves flash before the request).
11. Expiry (optional, if it occurs naturally): a 401 on Home or Search switches to the sign-in screen with the red "Your session expired" alert.

**Windows** (install `kaset-windows-nsis` from the latest green `App` run on hardware):

12. The installer, Start menu, and taskbar show the Kaset icon.
13. **Windows 11:** the sidebar shows Mica (a subtle wallpaper tint) and the log says `vibrancy=true`; the rest of the window is opaque. **Windows 10 (if available):** the sidebar is solid `#18181b`, the log says `vibrancy=false`, and no part of the window is see-through (if the desktop shows through anywhere, record where — the window may need `transparent` only on macOS via `tauri.macos.conf.json`).
14. Repeat 4, 5, 7, and 8 with Ctrl instead of ⌘ (Ctrl+→ next, Ctrl+1 Home, Ctrl+F Search). Check that the header, player bar, and dropdown blur render in WebView2.

## Self-review

- **Spec coverage (M4 "UI shell: sidebar, home, search, player bar"):** sidebar with Search, Home, and the account block with sign out (Tasks 4, 5, 8); routing (Task 8 `Shell`, Decision 2); Home with every shelf type the Swift parser supports, Quick picks grid and card carousels with artwork (Tasks 1, 8); search client call, parser, suggestions, explorer subcommands, and commands (Tasks 2, 3, 5); Search screen with the header field, debounced suggestions, chips, top result, and results by type, songs playable via `play_queue` (Task 7); persistent floating player bar wired to the existing commands/events (Task 8); TanStack Query for `get_home`/`search`/suggestions/account with stale times, deferred from M3 (Tasks 6–8); keyboard shortcuts for play/pause/next/prev following macOS conventions without rebinding system shortcuts, ⌘F checked against macOS conventions (Decision 8, Task 8); accessible markup (landmarks, labelled regions, ARIA combobox/listbox, `aria-current`, `aria-pressed`, `aria-expanded`, `aria-valuetext`, labelled play and carousel buttons, decorative artwork `alt=""`); plain CSS (Task 8); image CSP verified and widened to `*.ytimg.com` (Task 5). Human mockup requirements: dark theme, 240 px sidebar with `#d91c48` pill and account block, 56 px header, 3-column Quick picks, 6-column carousels with round artists and prev/next, Search header field with dropdown, chips, top result + songs, floating inset player bar with the requested arrangement and a queue button, sign-in expired alert (Decision 9, Tasks 7–8); CSS blur with `-webkit-` prefix on the bar, header, and dropdown (Decision 10); native sidebar vibrancy with a verified API and a solid fallback (Decision 11, Task 5). "React never sees cookies" holds (the account summary carries no credential). M2/M3 deferred UI items: see the table above. Not in M4 (spec): playlist/album detail (M5), CLI (M6). Not done from the mockup: a "Premium" label (no verified signal; Decision 12) and Korean chip labels (UI language is English until localization; Decision 9).
- **Placeholder scan:** every code step has complete code; no TBD/TODO. Task 4 Step 8 is an explicit human step, not a placeholder.
- **Type consistency:** `SearchItem`/`SearchResults` (Task 2) and `Account` (Task 4) are used by the Task 5 commands and appear in TS under the same names; `commands.search({ query })`, `commands.searchSuggestions({ input })`, `getAccount()`, `windowVibrancy()` mock signatures match the generated bindings; `queryKeys`, `createQueryClient`, `ACCOUNT_STALE_MS`, `useNotice`, `Artwork`, `ItemCard`, `isPlayable`, `toSong`, `subtitleOf`, `itemKey`, `shapeOf`, `usePlayQueue`, `renderWithProviders`, the `home`/`account`/`searchResults` fixtures (Task 6) are used with the same names and signatures in Tasks 7–8; `SearchScreen` props `query`/`onSubmit`/`focusRequest` (Task 7) match `Shell` (Task 8); `usePlayer(onError)` and `PlayerBar` match their tests; `window_effects::WindowEffects`/`apply` match `lib.rs` and `commands::window_vibrancy`.
- **Verified before writing:** all code in Tasks 1–9 was applied in task order to a fresh clone of this repository at `0fa5321` in a scratch directory and checked after each task: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` (and ytm-core with `--features specta`); `cargo test --workspace` (ytm-core: lib 16/17 with `specta`, `home_parser` 9 → 11, `search_parser` 11, `search_suggestions_parser` 2, `account_parser` 3, `client` 8 → 11 → 12; kaset 114 → 116); `bindings_are_current` regenerated the committed bindings; `pnpm lint`, `pnpm typecheck`, `pnpm test` (21 → 31 → 39 → 67 tests); `pnpm build`; `pnpm tauri build --bundles app` produced `Kaset.app` with the transparent dark window config, the new capability entries, CSP, and `icon.icns`; the Task 9 commands reproduced the committed `app-icon.png` byte for byte. The UI was rendered in Chromium through Vite with `mockCommands` (dark shell, sidebar pill and account block, Quick picks grid, carousels with round artists, floating player bar, header search with dropdown, chips, top result) to check the layout. External APIs read in source: tauri-utils 2.10.4 `WindowConfig.{transparent, theme, window_effects}` and `WindowEffect` (incl. `Sidebar`, `Mica`, Liquid Glass) and the `macOSPrivateApi` no-op note; tauri 2.12.7 `vibrancy/{macos,windows}.rs` (first-match application, no Windows fallback), `HasWindowHandle for WebviewWindow`, platform config files `tauri.macos.conf.json`/`tauri.windows.conf.json`; window-vibrancy 0.8.1 `apply_vibrancy`, `apply_mica`, `Error::UnsupportedPlatformVersion` (Windows build < 22000); npm metadata for @tanstack/react-query 5.107.1; TanStack Query v5 APIs used compile and run in the tests. **Not verified:** live search/suggestion/account/home responses (Task 4 Step 8); the `#[cfg(windows)]` branch of `window_effects::apply` (no Windows-capable toolchain locally; compiled by Windows CI in Task 5); native vibrancy, transparency, and CSS blur inside the Tauri WebViews (WKWebView, WebView2) and all other runtime behaviour (human checklist).
