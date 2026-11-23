import { createContext, useContext, useEffect, useState } from "react";
import { commands, events, type PlayerState, type QueueState } from "../bindings";
import { errorMessage } from "../errors";

export type PlayerView = {
  state: PlayerState | null;
  queue: QueueState | null;
};

export const PlayerStateContext = createContext<PlayerView["state"]>(null);
export const PlayerQueueContext = createContext<PlayerView["queue"]>(null);

export function usePlayerQueue() {
  return useContext(PlayerQueueContext);
}

export function usePlayer(): PlayerView {
  return { state: useContext(PlayerStateContext), queue: usePlayerQueue() };
}

/**
 * The player state and queue: `null` until known, then kept current by
 * `player://state` and `player://queue`. Events that arrive before the
 * initial `playerSnapshot` reply win. Pass a stable `onError` for listener
 * and snapshot failures.
 */
export function usePlayerSubscription(onError: (message: string) => void): PlayerView {
  const [state, setState] = useState<PlayerState | null>(null);
  const [queue, setQueue] = useState<QueueState | null>(null);

  useEffect(() => {
    let active = true;
    const stopState = events.playerState.listen((event) => {
      if (active) setState(event.payload.state);
    });
    const stopQueue = events.playerQueue.listen((event) => {
      if (active) setQueue(event.payload.queue);
    });
    async function initialize() {
      try {
        await Promise.all([stopState, stopQueue]);
      } catch (cause) {
        if (active) onError(`Could not listen for player updates: ${errorMessage(cause)}`);
        return;
      }
      if (!active) return;

      try {
        const snapshot = await commands.playerSnapshot();
        if (!active) return;
        setState((previous) => previous ?? snapshot.state);
        setQueue((previous) => previous ?? snapshot.queue);
      } catch (cause) {
        if (active) onError(`Could not read the player state: ${errorMessage(cause)}`);
      }
    }
    void initialize();
    return () => {
      active = false;
      for (const registration of [stopState, stopQueue]) {
        void registration.then((stop) => stop()).catch(() => {});
      }
    };
  }, [onError]);

  return { state, queue };
}
