import { QueryClientProvider, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import { SignInScreen } from "./auth/SignInScreen";
import { useAuthStatus } from "./auth/useAuthStatus";
import { Shell } from "./shell/Shell";

export function App({ queryClient }: { queryClient: QueryClient }) {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthGate />
    </QueryClientProvider>
  );
}

function AuthGate() {
  const queryClient = useQueryClient();
  const [session, setSession] = useState(0);
  const onSessionChange = useCallback(() => {
    queryClient.clear();
    setSession((current) => current + 1);
  }, [queryClient]);
  const { status, error, retry } = useAuthStatus(onSessionChange);
  const signedIn = status === "signed_in";

  // Cached feeds belong to the account that fetched them.
  useEffect(() => {
    if (!signedIn) queryClient.clear();
  }, [signedIn, queryClient]);

  if (status === null) {
    return (
      <main className="center">
        {error === null ? (
          <p>Loading…</p>
        ) : (
          <div role="alert">
            <p>Could not read the sign-in state: {error}</p>
            <button type="button" onClick={retry}>
              Try again
            </button>
          </div>
        )}
      </main>
    );
  }
  if (signedIn) return <Shell key={session} />;
  return <SignInScreen expired={status === "expired"} />;
}
