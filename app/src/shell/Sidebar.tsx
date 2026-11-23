import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type FeedKind, type LibraryKind } from "../bindings";
import { errorMessage } from "../errors";
import { ChartBarsIcon, ClockIcon, CompassIcon, HomeIcon, PaletteIcon, PodcastIcon, SearchIcon, SparkleIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { useNotice } from "../notices/NoticeProvider";
import { ACCOUNT_STALE_MS, queryKeys } from "../query";

type FeedRoute = { kind: FeedKind | "podcasts" };
type LibraryRoute = { kind: "library"; collection: LibraryKind };
type SearchRoute = { kind: "search" };
type CategoryRoute = { kind: "category"; browseId: string; params: string | null; title: string; from: Route };
type PageRoute = { kind: "page"; browseId: string; title: string; isAlbum: boolean; from: Route };
type PodcastShowRoute = { kind: "podcast_show"; browseId: string; title: string; from: Route };
type ArtistRoute = { kind: "artist"; browseId: string; name: string; from: Route };
type ArtistMoreRoute = { kind: "artist-more"; browseId: string; params: string | null; title: string; from: ArtistRoute };
export type Route = FeedRoute | LibraryRoute | SearchRoute | CategoryRoute | PageRoute | PodcastShowRoute | ArtistRoute | ArtistMoreRoute;

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

export const LIBRARY_TABS = {
  playlists: "Playlists",
  songs: "Liked songs",
  albums: "Albums",
  artists: "Artists",
} satisfies Record<LibraryKind, string>;

const LIBRARY_ORDER: LibraryKind[] = ["playlists", "songs", "albums", "artists"];

const FEED_ORDER: FeedRoute["kind"][] = ["home", "explore", "charts", "moods_and_genres", "new_releases", "podcasts", "history"];
const ROUTES = [SEARCH, ...FEED_ORDER.map((route) => ({ route, ...FEED_TABS[route] }))];

type Props = {
  route: Route;
  onNavigate: (route: Route) => void;
};

export function Sidebar({ route, onNavigate }: Props) {
  const { report, dismiss } = useNotice();
  const queryClient = useQueryClient();
  // Display only; a failure falls back to a generic label instead of an alert.
  const account = useQuery({
    queryKey: queryKeys.account,
    queryFn: () => commands.getAccount(),
    staleTime: ACCOUNT_STALE_MS,
  });

  const notifications = useQuery({
    queryKey: queryKeys.trackNotifications,
    queryFn: () => commands.trackNotifications(),
    staleTime: Infinity,
  });
  const saveNotifications = useMutation({
    mutationFn: (enabled: boolean) => commands.setTrackNotifications(enabled),
    onSuccess: (_, enabled) => queryClient.setQueryData(queryKeys.trackNotifications, enabled),
    onError: (cause) => report(errorMessage(cause)),
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
      <section aria-labelledby="sidebar-library">
        <h2 className="nav-heading" id="sidebar-library">Library</h2>
        <ul className="nav">
          {LIBRARY_ORDER.map((collection) => (
            <li key={collection}>
              <button
                type="button"
                aria-current={selected.kind === "library" && selected.collection === collection ? "page" : undefined}
                onClick={() => onNavigate({ kind: "library", collection })}
              >
                {LIBRARY_TABS[collection]}
              </button>
            </li>
          ))}
        </ul>
      </section>
      <section className="account" aria-label="Account">
        <Artwork url={account.data?.photo_url ?? null} size={32} shape="round" />
        <div className="item-text">
          <span className="item-title">{name}</span>
          {account.data?.handle && <span className="item-subtitle">{account.data.handle}</span>}
        </div>
        {notifications.data !== undefined ? (
          <label className="account-notifications">
            Notifications
            <input
              type="checkbox"
              className="visually-hidden"
              role="switch"
              checked={notifications.data}
              disabled={saveNotifications.isPending}
              onChange={(event) => saveNotifications.mutate(event.currentTarget.checked)}
            />
            <span className="account-switch-track" aria-hidden="true" />
          </label>
        ) : notifications.isError && (
          <button type="button" className="ghost" onClick={() => void notifications.refetch()}>
            Retry loading notifications
          </button>
        )}
        <button type="button" className="ghost" onClick={() => void signOut()}>
          Sign out
        </button>
      </section>
    </nav>
  );
}
