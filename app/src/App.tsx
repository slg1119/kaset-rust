import { SignInScreen } from "./auth/SignInScreen";
import { useAuthStatus } from "./auth/useAuthStatus";
import { HomeScreen } from "./home/HomeScreen";
import { PlayerBar } from "./player/PlayerBar";

export function App() {
  const status = useAuthStatus();
  if (status === null) {
    return (
      <main className="screen">
        <p>Loading…</p>
      </main>
    );
  }
  if (status === "signed_in") {
    return (
      <>
        <HomeScreen />
        <PlayerBar />
      </>
    );
  }
  return <SignInScreen expired={status === "expired"} />;
}
