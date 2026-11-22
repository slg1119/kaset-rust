import { useEffect, useState } from "react";
import { commands, events, type PlayerState, type QueueState } from "../bindings";
import { errorMessage } from "../errors";

export type PlayerView = {
  state: PlayerState | null;
  queue: QueueState | null;
};

/**
 * The player state and queue: `null` until known, then kept current by
 * `player://state` and `player://queue`. Events that arrive before the
 * initial `playerSnapshot` reply win. `onError` hears a failed snapshot;
 * pass a stable function.
 */
export function usePlayer(onError: (message: string) => void): PlayerView {
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
    commands.playerSnapshot().then(
      (snapshot) => {
        if (!active) return;
        setState((previous) => previous ?? snapshot.state);
        setQueue((previous) => previous ?? snapshot.queue);
      },
      (cause: unknown) => {
        if (active) onError(`Could not read the player state: ${errorMessage(cause)}`);
      },
    );
    return () => {
      active = false;
      void stopState.then((stop) => stop());
      void stopQueue.then((stop) => stop());
    };
  }, [onError]);

  return { state, queue };
}
