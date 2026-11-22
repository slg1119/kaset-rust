import { QueryClientProvider, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
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
  const { status, error, retry } = useAuthStatus();
  const queryClient = useQueryClient();
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
  if (signedIn) return <Shell />;
  return <SignInScreen expired={status === "expired"} />;
}
