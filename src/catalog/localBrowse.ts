import type { LocalMedia } from "./types";

export type LibraryMode = "local" | "history" | "favorites";
export type LibrarySort = "added" | "title" | "recent" | "progress";

const titles = new Intl.Collator("zh-CN", { numeric: true, sensitivity: "base" });

function progressPercent(item: LocalMedia): number {
  if (item.completed) return 100;
  const duration = item.durationSec ?? 0;
  if (!Number.isFinite(duration) || duration <= 0) return 0;
  return Math.max(0, Math.min(100, item.positionSec / duration * 100));
}

/** Pure view derivation; changing the display never changes library.json. */
export function browseLocalMedia(
  items: LocalMedia[], mode: LibraryMode, query: string, sort: LibrarySort,
): { total: number; visible: LocalMedia[] } {
  const base = items.filter(item =>
    mode === "history" ? item.lastPlayedMs !== null :
      mode === "favorites" ? item.favorite : true,
  );
  const needle = query.trim().toLocaleLowerCase();
  const visible = base
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
  return { total: base.length, visible };
}

/** One immutable list of playable IDs in the user's current displayed order. */
export function playlistFromVisible(visible: LocalMedia[]): number[] {
  return visible.filter(item => item.available).map(item => item.id);
}
