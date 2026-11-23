import { useCallback, useEffect, useState } from "react";
import { commands, events, type AuthStatus } from "../bindings";
import { errorMessage } from "../errors";

export type AuthView = {
  /** `null` until known, then kept current by `auth://changed`. */
  status: AuthStatus | null;
  /** Why the state could not be read; shown only while `status` is `null`. */
  error: string | null;
  retry: () => void;
};

/**
 * Subscribes to `auth://changed` before asking for the current state, so a
 * change between the reply and the subscription cannot be lost. An event that
 * arrives before the reply wins.
 */
export function useAuthStatus(onSessionChange?: () => void): AuthView {
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    events.authChanged
      .listen((event) => {
        if (!active) return;
        onSessionChange?.();
        setError(null);
        setStatus(event.payload.status);
      })
      .then((unlisten) => {
        if (!active) {
          unlisten();
          return;
        }
        stop = unlisten;
        return commands.authStatus().then((current) => {
          if (active) setStatus((previous) => previous ?? current);
        });
      })
      .catch((cause: unknown) => {
        if (active) setError(errorMessage(cause));
      });
    return () => {
      active = false;
      stop?.();
    };
  }, [attempt, onSessionChange]);

  const retry = useCallback(() => {
    setError(null);
    setAttempt((count) => count + 1);
  }, []);

  return { status, error, retry };
}
