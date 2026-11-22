import { useState } from "react";
import { commands } from "../bindings";
import { errorMessage } from "../errors";

type Props = {
  /** The previous session was rejected by YouTube Music (HTTP 401/403). */
  expired: boolean;
};

export function SignInScreen({ expired }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  async function signIn() {
    setError(null);
    setOpening(true);
    try {
      await commands.signIn();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setOpening(false);
    }
  }

  return (
    <main className="center sign-in">
      <h1>Kaset</h1>
      {expired ? (
        <div className="alert" role="alert">
          Your session expired. Sign in again to continue.
        </div>
      ) : (
        <p className="hint">Sign in with your Google account to use YouTube Music.</p>
      )}
      <button type="button" className="primary" onClick={signIn} disabled={opening}>
        Sign in with Google
      </button>
      {error && <p role="alert">{error}</p>}
    </main>
  );
}
