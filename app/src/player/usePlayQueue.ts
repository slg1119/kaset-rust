import { useCallback } from "react";
import { commands, type Song } from "../bindings";
import { errorMessage } from "../errors";
import { useNotice } from "../notices/NoticeProvider";

/** Plays `songs` from `songs[index]`; a failure goes to the app notice. */
export function usePlayQueue(): (songs: Song[], index: number) => Promise<void> {
  const { report, dismiss } = useNotice();
  return useCallback(
    async (songs: Song[], index: number) => {
      dismiss();
      try {
        await commands.playQueue(songs, index);
      } catch (cause) {
        report(errorMessage(cause));
      }
    },
    [report, dismiss],
  );
}
