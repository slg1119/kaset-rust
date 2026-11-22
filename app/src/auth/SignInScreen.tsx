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
    <main className="screen">
      <h1>Kaset</h1>
      <p>
        {expired
          ? "Your session expired. Sign in again to continue."
          : "Sign in with your Google account to use YouTube Music."}
      </p>
      <button type="button" onClick={signIn} disabled={opening}>
        Sign in with Google
      </button>
      {error && <p role="alert">{error}</p>}
    </main>
  );
}
