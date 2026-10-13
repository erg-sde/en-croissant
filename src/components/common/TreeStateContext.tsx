import { createContext, useEffect, useRef } from "react";
import { createTreeStore, type TreeStore } from "@/state/store/tree";
import { treeRegistry } from "@/state/store/treeRegistry";
import type { TreeState } from "@/utils/treeReducer";

export const TreeStateContext = createContext<TreeStore | null>(null);

export function TreeStateProvider({
  id,
  initial,
  children,
}: {
  id?: string;
  initial?: TreeState;
  children: React.ReactNode;
}) {
  const store = useRef(createTreeStore(id, initial)).current;

  useEffect(() => {
    if (!id) {
      return;
    }
    treeRegistry.register(id, store);
    return () => treeRegistry.unregister(id, store);
  }, [id, store]);

  return <TreeStateContext.Provider value={store}>{children}</TreeStateContext.Provider>;
}
