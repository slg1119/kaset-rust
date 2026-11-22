import { useEffect, useState } from "react";
import { commands, events, type AuthStatus } from "../bindings";

/**
 * The sign-in state: `null` until known, then kept current by `auth://changed`.
 * An event that arrives before the initial `authStatus` reply wins.
 */
export function useAuthStatus(): AuthStatus | null {
  const [status, setStatus] = useState<AuthStatus | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = events.authChanged.listen((event) => {
      if (active) setStatus(event.payload.status);
    });
    commands.authStatus().then(
      (current) => {
        if (active) setStatus((previous) => previous ?? current);
      },
      () => {
        if (active) setStatus((previous) => previous ?? "signed_out");
      },
    );
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return status;
}
