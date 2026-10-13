import { info, warn } from "@tauri-apps/plugin-log";
import { useEffect } from "react";
import { events } from "@/bindings";
import { treeRegistry } from "@/state/store/treeRegistry";
import { narratedGameRecord, narratedMoveRecord } from "@/utils/commentary/llm";
import type { ValidationResult } from "@/utils/commentary/types";

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
      if (!context) {
        warn(`Narration chunk for an unknown run ${payload.runId} ignored`);
        return;
      }
      // Counts and rule names only, never the model's text.
      let checked = 0;
      let dropped = 0;
      const rules = new Map<string, number>();
      const tally = (result: ValidationResult) => {
        checked += result.checked;
        dropped += result.dropped;
        for (const f of result.failures) rules.set(f.rule, (rules.get(f.rule) ?? 0) + 1);
      };
      const writes = payload.items
        .map((item) => narratedMoveRecord(context, item, payload.served, tally))
        .filter((write) => write !== null);
      const narrative = payload.narrative
        ? narratedGameRecord(context, payload.narrative, payload.served, tally)
        : null;
      info(
        `Narration chunk ${payload.chunk + 1}/${payload.chunks}: ${writes.length}/${payload.items.length} moves updated, narrative ${narrative ? "updated" : payload.narrative ? "kept (no sentence passed)" : "not in chunk"}, ${checked - dropped}/${checked} sentences passed${rules.size > 0 ? `, failures: ${[...rules].map(([rule, n]) => `${rule} ${n}`).join(", ")}` : ""}`,
      );
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
