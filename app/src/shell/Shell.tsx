import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { commands, type BrowseEndpoint, type FeedKind } from "../bindings";
import { ArtistMoreScreen, ArtistScreen } from "../detail/ArtistScreen";
import { BrowsePageScreen } from "../detail/BrowsePageScreen";
import { errorMessage } from "../errors";
import { FeedScreen, type CategoryItem } from "../home/FeedScreen";
import { LibraryScreen } from "../library/LibraryScreen";
import type { PageCard } from "../music/items";
import { NoticeBanner, NoticeProvider, useNotice } from "../notices/NoticeProvider";
import { PlayerProvider } from "../player/PlayerProvider";
import { PlayerBar } from "../player/PlayerBar";
import type { Filter } from "../search/groups";
import { SearchScreen } from "../search/SearchScreen";
import type { Shortcut } from "./shortcuts";
import { FEED_TABS, LIBRARY_TABS, Sidebar, type Route } from "./Sidebar";
import { useShortcuts } from "./useShortcuts";

/** The signed-in app: sidebar, the current screen, notices, and the floating player bar. */
export function Shell() {
  return (
    <NoticeProvider>
      <PlayerProvider><ShellLayout /></PlayerProvider>
    </NoticeProvider>
  );
}

function ShellLayout() {
  const [route, setRoute] = useState<Route>({ kind: "home" });
  // Kept here so leaving Search and coming back shows the same results.
  const [query, setQuery] = useState("");
  const [searchFilter, setSearchFilter] = useState<Filter>("all");
  // Bumped by ⌘F so the search field takes focus even when Search is open.
  const [searchFocus, setSearchFocus] = useState(0);
  const { report, dismiss } = useNotice();

  function openArtist(browseId: string, name: string) {
    setRoute((current) => current.kind === "artist" && current.browseId === browseId
      ? current
      : { kind: "artist", browseId, name, from: current });
  }

  function openPage(card: PageCard) {
    if (card.kind === "artist") {
      openArtist(card.browse_id, card.title);
      return;
    }
    setRoute({ kind: "page", browseId: card.browse_id, title: card.title, from: route });
  }

  function openCategory(item: CategoryItem) {
    setRoute({ kind: "category", browseId: item.browse_id, params: item.params, title: item.title, from: route });
  }

  function openArtistMore(endpoint: BrowseEndpoint, title: string) {
    if (route.kind === "artist") {
      setRoute({ kind: "artist-more", browseId: endpoint.browse_id, params: endpoint.params, title, from: route });
    }
  }

  const actions = useMemo<Partial<Record<Shortcut, () => void>>>(() => {
    const run = (command: () => Promise<unknown>) => {
      dismiss();
      command().catch((cause: unknown) => report(errorMessage(cause)));
    };
    return {
      togglePlayback: () => run(() => commands.togglePlayback()),
      nextTrack: () => run(() => commands.nextTrack()),
      previousTrack: () => run(() => commands.previousTrack()),
      goHome: () => setRoute({ kind: "home" }),
      goSearch: () => {
        setRoute({ kind: "search" });
        setSearchFocus((count) => count + 1);
      },
      back: "from" in route ? () => setRoute(route.from) : undefined,
    };
  }, [report, dismiss, route]);
  useShortcuts(actions);

  const artistActions = { onOpenPage: openPage, onOpenCategory: openCategory, onOpenArtist: openArtist };

  return (
    <div className="shell">
      <Sidebar route={route} onNavigate={setRoute} />
      <div className="content-column">
        {routeStack(route).map((entry, index) => {
          const active = entry === route;
          const onBack = "from" in entry ? () => setRoute(entry.from) : undefined;
          return (
            <ScreenFrame key={screenKey(entry, index)} active={active}>
              {(entry.kind === "category" || isFeedKind(entry.kind)) && (
                <FeedScreen
                  source={entry.kind === "category" ? entry : { kind: entry.kind }}
                  title={entry.kind === "category" ? entry.title : FEED_TABS[entry.kind].label}
                  {...artistActions}
                  active={active}
                  onBack={onBack}
                />
              )}
              {entry.kind === "search" && (
                <SearchScreen
                  query={query}
                  onSubmit={setQuery}
                  focusRequest={searchFocus}
                  filter={searchFilter}
                  onFilterChange={setSearchFilter}
                  onOpenPage={openPage}
                  onOpenArtist={openArtist}
                />
              )}
              {entry.kind === "library" && (entry.collection === "songs" ? (
                <BrowsePageScreen browseId="VLLM" title={LIBRARY_TABS.songs} onOpenArtist={openArtist} active={active} />
              ) : (
                <LibraryScreen collection={entry.collection} title={LIBRARY_TABS[entry.collection]} {...artistActions} />
              ))}
              {entry.kind === "page" && (
                <BrowsePageScreen browseId={entry.browseId} title={entry.title}
                  onOpenArtist={openArtist} onBack={onBack} active={active} />
              )}
              {entry.kind === "artist" && (
                <ArtistScreen browseId={entry.browseId} name={entry.name} {...artistActions}
                  onOpenMore={openArtistMore} onBack={() => setRoute(entry.from)} />
              )}
              {entry.kind === "artist-more" && (
                <ArtistMoreScreen browseId={entry.browseId} params={entry.params} title={entry.title}
                  {...artistActions} onBack={() => setRoute(entry.from)} />
              )}
            </ScreenFrame>
          );
        })}
        <NoticeBanner />
        <PlayerBar onOpenArtist={openArtist} />
      </div>
    </div>
  );
}

function isFeedKind(kind: Route["kind"]): kind is FeedKind {
  return Object.hasOwn(FEED_TABS, kind);
}

function routeStack(route: Route): Route[] {
  const routes = [route];
  while ("from" in route) {
    route = route.from;
    routes.push(route);
  }
  return routes.reverse();
}

function screenKey(route: Route, depth: number) {
  return JSON.stringify([
    depth, route.kind,
    "collection" in route ? route.collection : null,
    "browseId" in route ? route.browseId : null,
    "params" in route ? route.params : null,
  ]);
}

/** Covered screens keep their DOM and scroll containers while a detail is open. */
function ScreenFrame({ active, children }: { active: boolean; children: ReactNode }) {
  const lastControl = useRef<HTMLButtonElement | null>(null);
  useLayoutEffect(() => {
    if (active && lastControl.current?.isConnected) {
      lastControl.current.focus({ preventScroll: true });
    }
  }, [active]);

  return (
    <div
      className="content content-frame"
      role={active ? "main" : undefined}
      aria-hidden={!active || undefined}
      inert={!active}
      onClickCapture={(event) => {
        // macOS WebKit does not focus buttons clicked with a pointer.
        const target = event.target;
        if (target instanceof Element) lastControl.current = target.closest("button");
      }}
    >
      {children}
    </div>
  );
}
