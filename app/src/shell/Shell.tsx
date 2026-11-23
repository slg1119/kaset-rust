import { useMemo, useState } from "react";
import { commands, type BrowseEndpoint, type FeedKind } from "../bindings";
import { ArtistMoreScreen, ArtistScreen } from "../detail/ArtistScreen";
import { BrowsePageScreen } from "../detail/BrowsePageScreen";
import { errorMessage } from "../errors";
import { FeedScreen, type CategoryItem } from "../home/FeedScreen";
import type { PageCard } from "../music/items";
import { NoticeBanner, NoticeProvider, useNotice } from "../notices/NoticeProvider";
import { PlayerBar } from "../player/PlayerBar";
import type { Filter } from "../search/groups";
import { SearchScreen } from "../search/SearchScreen";
import type { Shortcut } from "./shortcuts";
import { FEED_TABS, Sidebar, type Route } from "./Sidebar";
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
    setRoute({ kind: "page", browseId: card.browse_id, title: card.title, isAlbum: card.kind === "album", from: route });
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
        <main key={JSON.stringify([route.kind, "browseId" in route ? route.browseId : null, "params" in route ? route.params : null])} className="content">
          {(route.kind === "category" || isFeedKind(route.kind)) && (
            <FeedScreen
              source={route.kind === "category" ? route : { kind: route.kind }}
              title={route.kind === "category" ? route.title : FEED_TABS[route.kind].label}
              {...artistActions}
              onBack={route.kind === "category" ? () => setRoute(route.from) : undefined}
            />
          )}
          {route.kind === "search" && (
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
          {route.kind === "page" && (
            <BrowsePageScreen
              browseId={route.browseId}
              title={route.title}
              isAlbum={route.isAlbum}
              onOpenArtist={openArtist}
              onBack={() => setRoute(route.from)}
            />
          )}
          {route.kind === "artist" && (
            <ArtistScreen browseId={route.browseId} name={route.name} {...artistActions}
              onOpenMore={openArtistMore} onBack={() => setRoute(route.from)} />
          )}
          {route.kind === "artist-more" && (
            <ArtistMoreScreen browseId={route.browseId} params={route.params} title={route.title}
              {...artistActions} onBack={() => setRoute(route.from)} />
          )}
        </main>
        <NoticeBanner />
        <PlayerBar onOpenArtist={openArtist} />
      </div>
    </div>
  );
}

function isFeedKind(kind: Route["kind"]): kind is FeedKind {
  return Object.hasOwn(FEED_TABS, kind);
}
