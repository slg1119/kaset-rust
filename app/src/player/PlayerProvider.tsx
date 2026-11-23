import type { ReactNode } from "react";
import { useNotice } from "../notices/NoticeProvider";
import { PlayerQueueContext, PlayerStateContext, usePlayerSubscription } from "./usePlayer";

export function PlayerProvider({ children }: { children: ReactNode }) {
  const { report } = useNotice();
  const { queue, state } = usePlayerSubscription(report);
  return (
    <PlayerStateContext.Provider value={state}>
      <PlayerQueueContext.Provider value={queue}>{children}</PlayerQueueContext.Provider>
    </PlayerStateContext.Provider>
  );
}
