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
