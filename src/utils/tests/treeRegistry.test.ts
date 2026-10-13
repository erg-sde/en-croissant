import { expect, test } from "vitest";
import { createTreeStore } from "@/state/store/tree";
import { treeRegistry } from "@/state/store/treeRegistry";

test("writes for a tab without a live store are parked and applied once, in order", () => {
    const order: string[] = [];
    treeRegistry.withStore("tab-parked", () => order.push("first"));
    treeRegistry.withStore("tab-parked", () => order.push("second"));
    expect(order).toStrictEqual([]);

    const store = createTreeStore();
    treeRegistry.register("tab-parked", store);
    expect(order).toStrictEqual(["first", "second"]);

    treeRegistry.unregister("tab-parked", store);
    treeRegistry.register("tab-parked", store);
    expect(order).toStrictEqual(["first", "second"]);
    treeRegistry.unregister("tab-parked", store);
});

test("writes go to the store that is live when they run, not when they were created", () => {
    const old = createTreeStore();
    const current = createTreeStore();
    treeRegistry.register("tab-live", old);
    treeRegistry.unregister("tab-live", old);
    treeRegistry.register("tab-live", current);

    let target = null;
    treeRegistry.withStore("tab-live", (store) => {
        target = store;
    });
    expect(target).toBe(current);
    treeRegistry.unregister("tab-live", current);
});

test("unregistering a replaced store keeps its successor", () => {
    const first = createTreeStore();
    const second = createTreeStore();
    treeRegistry.register("tab-replaced", first);
    treeRegistry.register("tab-replaced", second);
    treeRegistry.unregister("tab-replaced", first);
    expect(treeRegistry.get("tab-replaced")).toBe(second);
    treeRegistry.unregister("tab-replaced", second);
});

test("discarding a closed tab drops its parked writes", () => {
    let applied = false;
    treeRegistry.withStore("tab-closed", () => {
        applied = true;
    });
    treeRegistry.discard("tab-closed");
    treeRegistry.register("tab-closed", createTreeStore());
    expect(applied).toBe(false);
});
