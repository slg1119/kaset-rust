import { useMemo, useState } from "react";
import { commands } from "../bindings";
import { BrowsePageScreen } from "../detail/BrowsePageScreen";
import { errorMessage } from "../errors";
import { FeedScreen, type CategoryItem } from "../home/FeedScreen";
import type { OpenableItem } from "../music/items";
import { NoticeBanner, NoticeProvider, useNotice } from "../notices/NoticeProvider";
import { PlayerBar } from "../player/PlayerBar";
import { PodcastShowScreen } from "../podcasts/PodcastShowScreen";
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

  function openPage(card: OpenableItem) {
    if (route.kind === "page" || route.kind === "podcast_show") return;
    setRoute({ kind: card.kind === "podcast" ? "podcast_show" : "page", browseId: card.browse_id, title: card.title, from: route });
  }

  function openCategory(item: CategoryItem) {
    if (route.kind === "page" || route.kind === "podcast_show" || route.kind === "search") return;
    setRoute({ kind: "category", browseId: item.browse_id, params: item.params, title: item.title, from: route });
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
      back: route.kind === "page" || route.kind === "podcast_show" || route.kind === "category" ? () => setRoute(route.from) : undefined,
    };
  }, [report, dismiss, route]);
  useShortcuts(actions);

  return (
    <div className="shell">
      <Sidebar route={route} onNavigate={setRoute} />
      <div className="content-column">
        <main key={route.kind === "category" ? JSON.stringify([route.kind, route.browseId, route.params]) : route.kind} className="content">
          {route.kind !== "page" && route.kind !== "podcast_show" && route.kind !== "search" && (
            <FeedScreen
              source={route}
              title={route.kind === "category" ? route.title : FEED_TABS[route.kind].label}
              onOpenPage={openPage}
              onOpenCategory={openCategory}
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
            />
          )}
          {route.kind === "podcast_show" && (
            <PodcastShowScreen
              key={route.browseId}
              browseId={route.browseId}
              title={route.title}
              onBack={() => setRoute(route.from)}
            />
          )}
          {route.kind === "page" && (
            <BrowsePageScreen
              key={route.browseId}
              browseId={route.browseId}
              title={route.title}
              onBack={() => setRoute(route.from)}
            />
          )}
        </main>
        <NoticeBanner />
        <PlayerBar />
      </div>
    </div>
  );
}
