import { useSyncExternalStore } from "react";
import type { Detection } from "./detect";
import { readJson, writeJson } from "./storage";

export interface RecentSearch {
  query: string;
  kind: Exclude<Detection["kind"], "empty">;
  path: string;
  at: number;
}

const KEY = "sp.recentSearches";
const MAX = 8;

let recent: RecentSearch[] = readJson<RecentSearch[]>(KEY, []);
const listeners = new Set<() => void>();

function set(next: RecentSearch[]) {
  recent = next;
  writeJson(KEY, recent);
  listeners.forEach((l) => l());
}

export function addRecent(entry: Omit<RecentSearch, "at">): void {
  set([{ ...entry, at: Date.now() }, ...recent.filter((r) => r.path !== entry.path)].slice(0, MAX));
}

export function clearRecent(): void {
  set([]);
}

export function useRecent(): RecentSearch[] {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => recent,
  );
}
