import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { HomeIcon, SearchIcon } from "../icons";
import { Artwork } from "../music/Artwork";
import { useNotice } from "../notices/NoticeProvider";
import { ACCOUNT_STALE_MS, queryKeys } from "../query";

export type Route =
  | { kind: "home" }
  | { kind: "search" }
  | { kind: "page"; browseId: string; title: string; from: "home" | "search" };

const ROUTES = [
  { route: "search", label: "Search", Icon: SearchIcon },
  { route: "home", label: "Home", Icon: HomeIcon },
] as const;

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

  const selected = route.kind === "page" ? route.from : route.kind;
  const name = account.data?.name ?? "Signed in";
  return (
    <nav className="sidebar" aria-label="Main">
      <p className="app-name">Kaset</p>
      <ul className="nav">
        {ROUTES.map(({ route: destination, label, Icon }) => (
          <li key={destination}>
            <button
              type="button"
              aria-current={selected === destination ? "page" : undefined}
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
