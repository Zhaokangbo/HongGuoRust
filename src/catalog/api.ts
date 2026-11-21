import type { CatalogProvider, CatalogSearchPage, LocalMedia, Series, SeriesDetail } from "./types";

function desktopInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = window.__TAURI__?.core;
  if (!core) return Promise.reject(new Error("剧库仅能在 HongGuoRust 桌面客户端中使用"));
  return core.invoke<T>(command, args);
}

export function searchCatalog(query: string): Promise<Series[]> {
  return desktopInvoke<Series[]>("catalog_search", { query });
}

export function listCatalogSources(): Promise<CatalogProvider[]> {
  return desktopInvoke<CatalogProvider[]>("catalog_sources");
}

export function searchCatalogPage(
  query: string, sourceId: string, offset = 0, limit = 24,
): Promise<CatalogSearchPage> {
  return desktopInvoke<CatalogSearchPage>("catalog_search_page", {
    query, sourceId, offset, limit,
  });
}

export function getCatalogSeries(seriesId: string): Promise<SeriesDetail> {
  return desktopInvoke<SeriesDetail>("catalog_detail", { seriesId });
}

export function playCatalogEpisode(seriesId: string, episodeId: string): Promise<number> {
  return desktopInvoke<number>("catalog_play_episode", { seriesId, episodeId });
}

export function listLocalMedia(): Promise<LocalMedia[]> {
  return desktopInvoke<LocalMedia[]>("library_list");
}

export function addLocalMedia(): Promise<LocalMedia | null> {
  return desktopInvoke<LocalMedia | null>("library_add_local");
}

export function removeLocalMedia(id: number): Promise<void> {
  return desktopInvoke<void>("library_remove", { id });
}

export function clearLocalHistory(id: number): Promise<void> {
  return desktopInvoke<void>("library_clear_history", { id });
}

export function setLocalFavorite(id: number, favorite: boolean): Promise<void> {
  return desktopInvoke<void>("library_set_favorite", { id, favorite });
}
