import { useEffect, useState } from "react";
import { commands, type HomePage, type ShelfItem, type Song } from "../bindings";
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

/** The songs in a shelf, without the `kind` tag. */
function songsOf(items: ShelfItem[]): Song[] {
  return items.flatMap((item) =>
    item.kind === "song"
      ? [
          {
            video_id: item.video_id,
            title: item.title,
            artists: item.artists,
            thumbnail_url: item.thumbnail_url,
          },
        ]
      : [],
  );
}

export function HomeScreen() {
  const [state, setState] = useState<State>({ kind: "loading" });
  const [playError, setPlayError] = useState<string | null>(null);

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

  /** Plays the shelf's songs, starting with `songs[index]`. */
  async function play(songs: Song[], index: number) {
    setPlayError(null);
    try {
      await commands.playQueue(songs, index);
    } catch (cause) {
      setPlayError(errorMessage(cause));
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
      {playError && <p role="alert">{playError}</p>}
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
          <ul aria-label="Home sections" className="sections">
            {state.page.sections.map((section, index) => {
              const songs = songsOf(section.items);
              return (
                <li key={`${index}-${section.title}`}>
                  <h2>
                    {section.title} ({section.items.length})
                  </h2>
                  {songs.length > 0 && (
                    <ul aria-label={`${section.title} songs`} className="songs">
                      {songs.map((song, songIndex) => (
                        <li key={`${songIndex}-${song.video_id}`}>
                          <button type="button" onClick={() => void play(songs, songIndex)}>
                            {song.title}
                            {song.artists.length > 0 &&
                              ` · ${song.artists.map((artist) => artist.name).join(", ")}`}
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        ))}
    </main>
  );
}
