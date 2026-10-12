import { useEffect } from "react";
import { events } from "@/bindings";
import { treeRegistry } from "@/state/store/treeRegistry";
import { narratedGameRecord, narratedMoveRecord } from "@/utils/commentary/llm";

/**
 * Applies narration chunks as they arrive, to the live store of the tab they belong to (or parks
 * them until that tab is shown again). Mounted once in the root layout, so a chunk is never lost
 * because its board tab is not on screen. Chunks of a run this window does not know (after a
 * reload) are ignored; the deterministic commentary stays.
 */
export default function CommentaryListener() {
  useEffect(() => {
    const unlisten = events.narrationChunk.listen(({ payload }) => {
      const context = treeRegistry.run(payload.runId);
      if (!context) return;
      const writes = payload.items
        .map((item) => narratedMoveRecord(context, item, payload.served))
        .filter((write) => write !== null);
      const narrative = payload.narrative
        ? narratedGameRecord(context, payload.narrative, payload.served)
        : null;
      if (writes.length === 0 && !narrative) return;
      treeRegistry.withStore(context.tab, (live) =>
        live.getState().applyCommentary(context.runId, writes, narrative),
      );
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);
  return null;
}
