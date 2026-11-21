import type { Episode, SeriesDetail } from "../catalog/types";
import type { PlayerSnapshot } from "./snapshot";

/**
 * M10-B: a transient series-level navigation context, NOT M8's local file queue.
 * We only navigate the episodes returned by this exact scoped Provider detail.
 */
export interface EpisodeQueue {
  seriesId: string;
  seriesTitle: string;
  episodes: Episode[];
  index: number;
  sessionId: number;
}

export function createEpisodeQueue(
  detail: SeriesDetail, selectedEpisodeId: string, sessionId: number,
): EpisodeQueue {
  if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
    throw new Error("无效的原生播放器会话 ID");
  }
  const episodes = [...detail.episodes].sort((a, b) => a.number - b.number);
  if (new Set(episodes.map(item => item.id)).size !== episodes.length) {
    throw new Error("选集数据包含重复 ID");
  }
  const index = episodes.findIndex(item => item.id === selectedEpisodeId);
  if (index < 0 || !detail.series.id) {
    throw new Error("当前选集不属于该作品");
  }
  return {
    seriesId: detail.series.id,
    seriesTitle: detail.series.title,
    episodes,
    index,
    sessionId,
  };
}

export function adjacentEpisode(
  queue: EpisodeQueue | null, delta: -1 | 1,
): { index: number; episode: Episode } | null {
  if (!queue) return null;
  const index = queue.index + delta;
  const episode = queue.episodes[index];
  return index >= 0 && episode ? { index, episode } : null;
}

export function currentEpisodeEnded(
  queue: EpisodeQueue | null, snapshot: PlayerSnapshot | null,
): boolean {
  return Boolean(
    queue
    && snapshot?.state === "Ended"
    && snapshot.sessionId === queue.sessionId
    && Number.isSafeInteger(queue.sessionId)
    && queue.sessionId > 0,
  );
}

export function currentEpisodeFailed(
  queue: EpisodeQueue | null, snapshot: PlayerSnapshot | null,
): boolean {
  return Boolean(
    queue
    && snapshot?.state === "Error"
    && snapshot.sessionId === queue.sessionId,
  );
}
