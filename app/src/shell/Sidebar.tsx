import { useQuery } from "@tanstack/react-query";
import { commands, type FeedKind } from "../bindings";
import { errorMessage } from "../errors";
import { ChartBarsIcon, ClockIcon, CompassIcon, HomeIcon, PaletteIcon, PodcastIcon, SearchIcon, SparkleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { useNotice } from "../notices/NoticeProvider";
import { ACCOUNT_STALE_MS, queryKeys } from "../query";

type FeedRoute = { kind: FeedKind | "podcasts" };
type SearchRoute = { kind: "search" };
type CategoryRoute = { kind: "category"; browseId: string; params: string | null; title: string; from: Route };
type PageRoute = { kind: "page"; browseId: string; title: string; from: Route };
type PodcastShowRoute = { kind: "podcast_show"; browseId: string; title: string; from: Route };
type ArtistRoute = { kind: "artist"; browseId: string; name: string; from: Route };
type ArtistMoreRoute = { kind: "artist-more"; browseId: string; params: string | null; title: string; from: ArtistRoute };
export type Route = FeedRoute | SearchRoute | CategoryRoute | PageRoute | PodcastShowRoute | ArtistRoute | ArtistMoreRoute;

const SEARCH = { route: "search", label: "Search", Icon: SearchIcon } as const;

export const FEED_TABS = {
  home: { label: "Home", Icon: HomeIcon },
  explore: { label: "Explore", Icon: CompassIcon },
  charts: { label: "Charts", Icon: ChartBarsIcon },
  moods_and_genres: { label: "Moods & Genres", Icon: PaletteIcon },
  new_releases: { label: "New releases", Icon: SparkleIcon },
  podcasts: { label: "Podcasts", Icon: PodcastIcon },
  history: { label: "History", Icon: ClockIcon },
} satisfies Record<FeedRoute["kind"], { label: string; Icon: typeof HomeIcon }>;

const FEED_ORDER: FeedRoute["kind"][] = ["home", "explore", "charts", "moods_and_genres", "new_releases", "podcasts", "history"];
const ROUTES = [SEARCH, ...FEED_ORDER.map((route) => ({ route, ...FEED_TABS[route] }))];

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

  let selected = route;
  while ("from" in selected) selected = selected.from;
  const name = account.data?.name ?? "Signed in";
  return (
    <nav className="sidebar" aria-label="Main">
      <p className="app-name">Kaset</p>
      <ul className="nav">
        {ROUTES.map(({ route: destination, label, Icon }) => (
          <li key={destination}>
            <button
              type="button"
              aria-current={selected.kind === destination ? "page" : undefined}
              onClick={() => onNavigate({ kind: destination })}
            >
              <Icon size={18} />
              {label}
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
