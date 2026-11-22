import { useEffect, useState } from "react";
import { commands, type HomePage } from "../bindings";
import { errorMessage } from "../errors";

type State =
  | { kind: "loading" }
  | { kind: "loaded"; page: HomePage }
  | { kind: "error"; message: string };

function fetchHome(): Promise<State> {
  return commands.getHome().then(
    (page): State => ({ kind: "loaded", page }),
    // An auth_expired error also arrives as auth://changed, which replaces this screen.
    (cause: unknown): State => ({ kind: "error", message: errorMessage(cause) }),
  );
}

export function HomeScreen() {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let active = true;
    void fetchHome().then((next) => {
      if (active) setState(next);
    });
    return () => {
      active = false;
    };
  }, []);

  function retry() {
    setState({ kind: "loading" });
    void fetchHome().then(setState);
  }

  async function signOut() {
    try {
      await commands.signOut();
    } catch (cause) {
      setState({ kind: "error", message: errorMessage(cause) });
    }
  }

  return (
    <main className="screen">
      <header className="toolbar">
        <h1>Home</h1>
        <button type="button" onClick={signOut}>
          Sign out
        </button>
      </header>
      {state.kind === "loading" && <p>Loading…</p>}
      {state.kind === "error" && (
        <div role="alert">
          <p>{state.message}</p>
          <button type="button" onClick={retry}>
            Retry
          </button>
        </div>
      )}
      {state.kind === "loaded" &&
        (state.page.sections.length === 0 ? (
          <p>No sections.</p>
        ) : (
          <ul aria-label="Home sections">
            {state.page.sections.map((section, index) => (
              <li key={`${index}-${section.title}`}>
                {section.title} ({section.items.length})
              </li>
            ))}
          </ul>
        ))}
    </main>
  );
}
