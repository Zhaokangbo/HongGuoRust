import type { Episode, ProviderEpisodeProgress } from "./types";

export function latestEpisodeProgress(
  episodes: Episode[], items: ProviderEpisodeProgress[],
): ProviderEpisodeProgress | null {
  const validIds = new Set(episodes.map(episode => episode.id));
  return items.filter(item =>
    validIds.has(item.episodeId)
    && Number.isFinite(item.lastPlayedMs)
    && item.lastPlayedMs > 0
    && Number.isFinite(item.positionSec)
    && item.positionSec >= 0
  ).sort((a, b) => b.lastPlayedMs - a.lastPlayedMs)[0] ?? null;
}

export function formatEpisodePosition(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "--:--";
  const s = Math.floor(seconds);
  return String(Math.floor(s / 60)).padStart(2, "0") + ":" + String(s % 60).padStart(2, "0");
}
