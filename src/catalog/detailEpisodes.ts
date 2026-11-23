import type { Episode, ProviderEpisodeProgress } from "./types";

export type EpisodeStatusFilter = "all" | "incomplete" | "completed" | "unavailable";
export type EpisodeOrder = "ascending" | "descending";

export interface EpisodeListItem {
  episode: Episode;
  progress: ProviderEpisodeProgress | null;
  percent: number | null;
}

export interface EpisodeProgressSummary {
  total: number;
  completed: number;
  inProgress: number;
  unavailable: number;
  percent: number;
}

function episodeProgressPercent(positionSec: number, durationSec: number | null, completed: boolean): number | null {
  if (completed) return 100;
  if (!Number.isFinite(positionSec) || positionSec < 0
      || durationSec === null || !Number.isFinite(durationSec) || durationSec <= 0) return null;
  return Math.round(Math.min(100, Math.max(0, positionSec / durationSec * 100)));
}

function normalizeQuery(value: string): string {
  return value.normalize("NFKC").toLocaleLowerCase("zh-CN").replace(/\s+/g, "");
}

function validProgress(item: ProviderEpisodeProgress, seriesId: string): boolean {
  return item.seriesId === seriesId
    && typeof item.episodeId === "string"
    && item.episodeId.length > 0
    && Number.isFinite(item.lastPlayedMs)
    && item.lastPlayedMs > 0
    && Number.isFinite(item.positionSec)
    && item.positionSec >= 0;
}

export function filterEpisodes(
  episodes: readonly Episode[],
  progress: readonly ProviderEpisodeProgress[],
  seriesId: string,
  query: string,
  filter: EpisodeStatusFilter,
  order: EpisodeOrder,
): EpisodeListItem[] {
  const byId = new Map<string, ProviderEpisodeProgress>();
  for (const item of progress) {
    if (validProgress(item, seriesId)) byId.set(item.episodeId, item);
  }
  const term = normalizeQuery(query);
  const numberQuery = term.match(/^(?:第)?(\d+)(?:集)?$/);
  const requestedEpisodeNumber = numberQuery && Number.isSafeInteger(Number(numberQuery[1]))
    ? Number(numberQuery[1]) : null;
  const rows = episodes.map(episode => {
    const saved = byId.get(episode.id) ?? null;
    return {
      episode,
      progress: saved,
      percent: saved
        ? episodeProgressPercent(saved.positionSec, saved.durationSec, saved.completed)
        : null,
    };
  }).filter(row => {
    const { episode, progress: saved } = row;
    if (filter === "completed" && !saved?.completed) return false;
    if (filter === "incomplete" && saved?.completed) return false;
    if (filter === "unavailable" && episode.playable !== false) return false;
    if (!term) return true;
    if (requestedEpisodeNumber !== null) return episode.number === requestedEpisodeNumber;
    return normalizeQuery(episode.title).includes(term);
  });
  rows.sort((a, b) => {
    const delta = a.episode.number - b.episode.number;
    return (order === "descending" ? -delta : delta)
      || a.episode.id.localeCompare(b.episode.id);
  });
  return rows;
}

export function summarizeEpisodes(
  episodes: readonly Episode[],
  progress: readonly ProviderEpisodeProgress[],
  seriesId: string,
): EpisodeProgressSummary {
  const byId = new Map<string, ProviderEpisodeProgress>();
  for (const item of progress) {
    if (validProgress(item, seriesId)) byId.set(item.episodeId, item);
  }
  const completed = episodes.filter(episode => byId.get(episode.id)?.completed).length;
  const inProgress = episodes.filter(episode => {
    const saved = byId.get(episode.id);
    return Boolean(saved && !saved.completed && saved.positionSec > 0);
  }).length;
  const unavailable = episodes.filter(episode => episode.playable === false).length;
  return {
    total: episodes.length,
    completed,
    inProgress,
    unavailable,
    percent: episodes.length ? Math.round(completed / episodes.length * 100) : 0,
  };
}
