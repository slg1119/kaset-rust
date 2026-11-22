import { useEffect, useState } from "react";
import { commands, events, type PlayerState, type QueueState } from "../bindings";

export type PlayerView = {
  state: PlayerState | null;
  queue: QueueState | null;
};

/**
 * The player state and queue: `null` until known, then kept current by
 * `player://state` and `player://queue`. Events that arrive before the
 * initial `playerSnapshot` reply win.
 */
export function usePlayer(): PlayerView {
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
      () => {
        // No snapshot: the bar stays hidden until the first event.
      },
    );
    return () => {
      active = false;
      void stopState.then((stop) => stop());
      void stopQueue.then((stop) => stop());
    };
  }, []);

  return { state, queue };
}
