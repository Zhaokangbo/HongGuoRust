import type { LocalMedia } from "./types";

export type LibraryMode = "local" | "history" | "favorites";
export type LibrarySort = "added" | "title" | "recent" | "progress";
export type LibraryFilter = "all" | "available" | "missing" | "in-progress" | "completed" | "unplayed";

export interface LocalLibrarySummary {
  total: number;
  available: number;
  missing: number;
  inProgress: number;
  completed: number;
  favorites: number;
}

const titles = new Intl.Collator("zh-CN", { numeric: true, sensitivity: "base" });

function progressPercent(item: LocalMedia): number {
  if (item.completed) return 100;
  const duration = item.durationSec ?? 0;
  if (!Number.isFinite(duration) || duration <= 0) return 0;
  const position = Number.isFinite(item.positionSec) ? Math.max(0, item.positionSec) : 0;
  return Math.max(0, Math.min(100, position / duration * 100));
}

function inProgress(item: LocalMedia): boolean {
  return item.lastPlayedMs !== null && !item.completed;
}

function matchesFilter(item: LocalMedia, filter: LibraryFilter): boolean {
  switch (filter) {
    case "available": return item.available;
    case "missing": return !item.available;
    case "in-progress": return inProgress(item);
    case "completed": return item.lastPlayedMs !== null && item.completed;
    case "unplayed": return item.lastPlayedMs === null;
    case "all": return true;
  }
}

function itemsForMode(items: readonly LocalMedia[], mode: LibraryMode): LocalMedia[] {
  return items.filter(item =>
    mode === "history" ? item.lastPlayedMs !== null :
      mode === "favorites" ? item.favorite : true,
  );
}

/** Counts are derived from existing local-library records; no disk data is changed. */
export function localLibrarySummary(
  items: readonly LocalMedia[], mode: LibraryMode,
): LocalLibrarySummary {
  const base = itemsForMode(items, mode);
  return {
    total: base.length,
    available: base.filter(item => item.available).length,
    missing: base.filter(item => !item.available).length,
    inProgress: base.filter(inProgress).length,
    completed: base.filter(item => item.lastPlayedMs !== null && item.completed).length,
    favorites: items.filter(item => item.favorite).length,
  };
}

/** Pure view derivation; changing the display never changes local-library.json. */
export function browseLocalMedia(
  items: readonly LocalMedia[],
  mode: LibraryMode,
  query: string,
  sort: LibrarySort,
  filter: LibraryFilter = "all",
): { total: number; matched: number; visible: LocalMedia[] } {
  const base = itemsForMode(items, mode);
  const filtered = base.filter(item => matchesFilter(item, filter));
  const needle = query.trim().toLocaleLowerCase();
  const visible = filtered
    .filter(item => !needle ||
      item.title.toLocaleLowerCase().includes(needle) ||
      item.filename.toLocaleLowerCase().includes(needle))
    .sort((a, b) => {
      switch (sort) {
        case "title":
          return titles.compare(a.title, b.title) || b.id - a.id;
        case "recent":
          return (b.lastPlayedMs ?? 0) - (a.lastPlayedMs ?? 0) || b.id - a.id;
        case "progress":
          return progressPercent(b) - progressPercent(a) || b.id - a.id;
        case "added":
          return b.id - a.id;
      }
    });
  return { total: base.length, matched: filtered.length, visible };
}

/** One immutable list of playable IDs in the user's current displayed order. */
export function playlistFromVisible(visible: readonly LocalMedia[]): number[] {
  return visible.filter(item => item.available).map(item => item.id);
}
