import type { NarrationRunContext } from "@/utils/commentary/llm";
import type { TreeStore } from "./tree";

/** A write that needs a tab's live store. */
export type ParkedWrite = (store: TreeStore) => void;

const live = new Map<string, TreeStore>();
const parked = new Map<string, ParkedWrite[]>();
/** Narration runs by run id, newest last; a few are kept so late chunks still find theirs. */
const runs = new Map<string, NarrationRunContext>();
const MAX_RUNS = 4;

/**
 * The live tree store of each board tab, so results of long-running jobs are written to the store
 * the user sees rather than to the one that existed when the job started. A board tab's store is
 * destroyed when the tab is switched away from and recreated from session storage when it returns;
 * writes for a tab without a live store are parked and applied, in order, when it is registered.
 */
export const treeRegistry = {
    register(tab: string, store: TreeStore) {
        live.set(tab, store);
        const queue = parked.get(tab);
        if (queue) {
            parked.delete(tab);
            for (const write of queue) {
                write(store);
            }
        }
    },

    unregister(tab: string, store: TreeStore) {
        if (live.get(tab) === store) {
            live.delete(tab);
        }
    },

    get(tab: string): TreeStore | null {
        return live.get(tab) ?? null;
    },

    /** Runs `write` on the tab's live store now, or when the tab's store is next registered. */
    withStore(tab: string, write: ParkedWrite) {
        const store = live.get(tab);
        if (store) {
            write(store);
        } else {
            parked.set(tab, [...(parked.get(tab) ?? []), write]);
        }
    },

    /** Drops the writes parked for a tab that was closed. */
    discard(tab: string) {
        parked.delete(tab);
    },

    /** Records what a narration run needs to turn its chunks into records. */
    startRun(context: NarrationRunContext) {
        runs.delete(context.runId);
        runs.set(context.runId, context);
        while (runs.size > MAX_RUNS) {
            runs.delete(runs.keys().next().value!);
        }
    },

    /** The context of a narration run, or `null` after a reload or once it is too old. */
    run(runId: string): NarrationRunContext | null {
        return runs.get(runId) ?? null;
    },

    /** Whether any narration run is known for `tab` (empty after a reload). */
    hasRun(tab: string): boolean {
        return [...runs.values()].some((run) => run.tab === tab);
    },
};
