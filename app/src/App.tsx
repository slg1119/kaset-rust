import { SignInScreen } from "./auth/SignInScreen";
import { useAuthStatus } from "./auth/useAuthStatus";
import { HomeScreen } from "./home/HomeScreen";

export function App() {
  const status = useAuthStatus();
  if (status === null) {
    return (
      <main className="screen">
        <p>Loading…</p>
      </main>
    );
  }
  if (status === "signed_in") return <HomeScreen />;
  return <SignInScreen expired={status === "expired"} />;
}
