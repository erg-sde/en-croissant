import { warn } from "@tauri-apps/plugin-log";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { createDebouncedSessionStorage, writeSessionState } from "@/state/store/debouncedStorage";

vi.mock("@tauri-apps/plugin-log", () => ({ warn: vi.fn() }));

const state = (n: number) => ({ version: 0, state: { n } });

beforeEach(() => {
    sessionStorage.clear();
    vi.mocked(warn).mockClear();
});

afterEach(() => {
    vi.restoreAllMocks();
});

function failWritesTo(name: string) {
    const setItem = Storage.prototype.setItem;
    return vi.spyOn(Storage.prototype, "setItem").mockImplementation(function (
        this: Storage,
        key: string,
        value: string,
    ) {
        if (key === name) {
            throw new DOMException("The quota has been exceeded.", "QuotaExceededError");
        }
        setItem.call(this, key, value);
    });
}

test("a failing key does not stop the other keys from being written", () => {
    failWritesTo("full");
    writeSessionState("a", state(1));
    writeSessionState("full", state(2));
    writeSessionState("b", state(3));

    expect(JSON.parse(sessionStorage.getItem("a")!)).toStrictEqual(state(1));
    expect(JSON.parse(sessionStorage.getItem("b")!)).toStrictEqual(state(3));
    expect(sessionStorage.getItem("full")).toBeNull();
    expect(warn).toHaveBeenCalledTimes(1);
});

test("a failed write keeps serving reads from memory and is retried", () => {
    const storage = createDebouncedSessionStorage<{ n: number }>();
    const spy = failWritesTo("full");
    writeSessionState("full", state(7));
    expect(storage.getItem("full")).toStrictEqual(state(7));

    spy.mockRestore();
    writeSessionState("other", state(8));
    expect(JSON.parse(sessionStorage.getItem("full")!)).toStrictEqual(state(7));
});
