import type { LocalMedia, RecentCatalogEpisode } from "./types";

export type RecentActivityItem =
  | { kind: "provider"; key: string; lastPlayedMs: number; value: RecentCatalogEpisode }
  | { kind: "local"; key: string; lastPlayedMs: number; value: LocalMedia };

function validDate(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value)
    && value > 0 && value <= 8.64e15;
}

/** Merge local M7 history with validated C1 Provider history, newest first. */
export function combineRecentActivity(
  providers: readonly RecentCatalogEpisode[],
  locals: readonly LocalMedia[],
  limit: number,
): RecentActivityItem[] {
  const items: RecentActivityItem[] = [];
  const seen = new Set<string>();
  for (const entry of providers) {
    const { series, episode, progress } = entry;
    if (!series.id || !episode.id || progress.seriesId !== series.id
        || progress.episodeId !== episode.id || !validDate(progress.lastPlayedMs)) continue;
    const key = "provider:" + series.id;
    if (seen.has(key)) continue;
    seen.add(key);
    items.push({ kind: "provider", key, lastPlayedMs: progress.lastPlayedMs, value: entry });
  }
  for (const entry of locals) {
    if (!Number.isSafeInteger(entry.id) || entry.id <= 0 || !validDate(entry.lastPlayedMs)) continue;
    const key = "local:" + entry.id;
    if (seen.has(key)) continue;
    seen.add(key);
    items.push({ kind: "local", key, lastPlayedMs: entry.lastPlayedMs!, value: entry });
  }
  items.sort((a, b) => b.lastPlayedMs - a.lastPlayedMs || a.key.localeCompare(b.key));
  return items.slice(0, Math.max(0, Math.floor(limit)));
}

export function watchedEpisodeCount(
  episodeIds: readonly string[], played: readonly { episodeId: string; lastPlayedMs: number }[],
): number {
  const ids = new Set(episodeIds);
  return new Set(played.filter(item => ids.has(item.episodeId) && validDate(item.lastPlayedMs))
    .map(item => item.episodeId)).size;
}

export function watchedPercent(positionSec: number, durationSec: number | null, completed: boolean): number | null {
  if (completed) return 100;
  if (!Number.isFinite(positionSec) || positionSec < 0
      || durationSec === null || !Number.isFinite(durationSec) || durationSec <= 0) return null;
  return Math.round(Math.min(100, Math.max(0, positionSec / durationSec * 100)));
}
