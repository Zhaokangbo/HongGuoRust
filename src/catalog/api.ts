import type { Series, SeriesDetail } from "./types";

function desktopInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = window.__TAURI__?.core;
  if (!core) return Promise.reject(new Error("剧库仅能在 HongGuoRust 桌面客户端中使用"));
  return core.invoke<T>(command, args);
}

export function searchCatalog(query: string): Promise<Series[]> {
  return desktopInvoke<Series[]>("catalog_search", { query });
}

export function getCatalogSeries(seriesId: string): Promise<SeriesDetail> {
  return desktopInvoke<SeriesDetail>("catalog_detail", { seriesId });
}

export function playCatalogEpisode(seriesId: string, episodeId: string): Promise<number> {
  return desktopInvoke<number>("catalog_play_episode", { seriesId, episodeId });
}
