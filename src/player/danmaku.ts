export interface DanmakuItem {
  id: string;
  text: string;
  timeSec: number;
  color: string;
}

export interface QualityOption {
  quality: number;
  label: string;
  codec: string;
  selected: boolean;
}

export async function fetchEpisodeDanmaku(
  seriesId: string,
  episodeId: string,
  durationMs?: number,
): Promise<DanmakuItem[]> {
  const core = window.__TAURI__?.core;
  if (!core) return [];
  const payload: Record<string, unknown> = { seriesId, episodeId };
  if (typeof durationMs === "number" && Number.isFinite(durationMs) && durationMs > 0) {
    payload.durationMs = Math.round(durationMs);
  }
  const rows = await core.invoke<DanmakuItem[]>("catalog_episode_danmaku", payload);
  return Array.isArray(rows) ? rows : [];
}

export async function fetchEpisodeQualities(
  seriesId: string,
  episodeId: string,
): Promise<QualityOption[]> {
  const core = window.__TAURI__?.core;
  if (!core) return [];
  const rows = await core.invoke<QualityOption[]>("catalog_episode_qualities", {
    seriesId,
    episodeId,
  });
  return Array.isArray(rows) ? rows : [];
}

export interface ActiveDanmaku {
  key: string;
  text: string;
  color: string;
  lane: number;
  durationSec: number;
}

export const DANMAKU_LANES = 8;

export function pickDanmakuLane(laneFreeAt: number[], nowMs: number): number {
  let bestLane = 0;
  let earliest = laneFreeAt[0] ?? 0;
  for (let i = 0; i < laneFreeAt.length; i++) {
    if (laneFreeAt[i] <= nowMs) return i;
    if (laneFreeAt[i] < earliest) {
      earliest = laneFreeAt[i];
      bestLane = i;
    }
  }
  return bestLane;
}
