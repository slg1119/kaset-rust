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
