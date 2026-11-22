import { useMemo, useState } from "react";
import { commands, type BrowseCard } from "../bindings";
import { BrowsePageScreen } from "../detail/BrowsePageScreen";
import { errorMessage } from "../errors";
import { HomeScreen } from "../home/HomeScreen";
import { NoticeBanner, NoticeProvider, useNotice } from "../notices/NoticeProvider";
import { PlayerBar } from "../player/PlayerBar";
import type { Filter } from "../search/groups";
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
  const [route, setRoute] = useState<Route>({ kind: "home" });
  // Kept here so leaving Search and coming back shows the same results.
  const [query, setQuery] = useState("");
  const [searchFilter, setSearchFilter] = useState<Filter>("all");
  // Bumped by ⌘F so the search field takes focus even when Search is open.
  const [searchFocus, setSearchFocus] = useState(0);
  const { report, dismiss } = useNotice();

  function openPage(card: BrowseCard) {
    if (route.kind === "page") return;
    setRoute({ kind: "page", browseId: card.browse_id, title: card.title, from: route.kind });
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
      back: route.kind === "page" ? () => setRoute({ kind: route.from }) : undefined,
    };
  }, [report, dismiss, route]);
  useShortcuts(actions);

  return (
    <div className="shell">
      <Sidebar route={route} onNavigate={setRoute} />
      <div className="content-column">
        <main key={route.kind} className="content">
          {route.kind === "home" && <HomeScreen onOpenPage={openPage} />}
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
          {route.kind === "page" && (
            <BrowsePageScreen
              key={route.browseId}
              browseId={route.browseId}
              title={route.title}
              onBack={() => setRoute({ kind: route.from })}
            />
          )}
        </main>
        <NoticeBanner />
        <PlayerBar />
      </div>
    </div>
  );
}
