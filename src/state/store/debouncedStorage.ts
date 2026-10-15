import { warn } from "@tauri-apps/plugin-log";
import { type PersistStorage, type StorageValue } from "zustand/middleware";

const DEBOUNCE_MS = 300;
const pendingWrites = new Map<string, StorageValue<unknown>>();

let flushTimeout: ReturnType<typeof setTimeout> | null = null;
let flushHandlersBound = false;
let failureReported = false;

// Writes every pending entry. An entry that cannot be written (typically a QuotaExceededError) stays
// pending, so the in-memory state keeps serving reads and the write is retried on the next flush;
// the other entries are still written. The first failure of a run is logged.
function flush() {
    if (pendingWrites.size === 0) {
        return;
    }

    let failed = false;
    for (const [name, value] of pendingWrites) {
        try {
            sessionStorage.setItem(name, JSON.stringify(value));
            pendingWrites.delete(name);
        } catch (e) {
            failed = true;
            if (!failureReported) {
                failureReported = true;
                warn(`Could not save tab state "${name}" to session storage: ${e}`);
            }
        }
    }

    if (!failed) {
        failureReported = false;
    }
}

/** Writes a tab's persisted state immediately, outside a store, with the same failure handling. */
export function writeSessionState(name: string, value: StorageValue<unknown>) {
    pendingWrites.set(name, value);
    flush();
}

function scheduleFlush(delay: number) {
    if (flushTimeout) {
        clearTimeout(flushTimeout);
    }

    flushTimeout = setTimeout(() => {
        flushTimeout = null;
        flush();
    }, delay);
}

function bindFlushHandlers() {
    if (flushHandlersBound || typeof window === "undefined") {
        return;
    }

    const flushAndClearTimeout = () => {
        if (flushTimeout) {
            clearTimeout(flushTimeout);
            flushTimeout = null;
        }

        flush();
    };

    window.addEventListener("beforeunload", flushAndClearTimeout);
    window.addEventListener("pagehide", flushAndClearTimeout);

    flushHandlersBound = true;
}

export function createDebouncedSessionStorage<S>(delay = DEBOUNCE_MS): PersistStorage<S> {
    bindFlushHandlers();

    return {
        getItem: (name) => {
            const pending = pendingWrites.get(name);
            if (pending) {
                return pending as StorageValue<S>;
            }

            const stored = sessionStorage.getItem(name);
            return stored ? (JSON.parse(stored) as StorageValue<S>) : null;
        },
        setItem: (name, value) => {
            pendingWrites.set(name, value as StorageValue<unknown>);
            scheduleFlush(delay);
        },
        removeItem: (name) => {
            pendingWrites.delete(name);
            sessionStorage.removeItem(name);
        },
    };
}
