import type {
  CatalogProvider, CatalogSearchPage, FavoriteSeries, LocalMedia, ManifestSubscriptionInfo,
  ProviderEpisodeProgress, RecentCatalogEpisode, Series, SeriesDetail, WebDavStatus,
} from "./types";

export interface CatalogRequestOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
}

let nextCatalogRequestId = 0;

function desktopInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = window.__TAURI__?.core;
  if (!core) return Promise.reject(new Error("剧库仅能在 HongGuoRust 桌面客户端中使用"));
  return core.invoke<T>(command, args);
}

/**
 * Abort releases the UI immediately and signals the Rust request registry.
 * A future network Provider must also check the cooperative request token and
 * configure its own bounded I/O; a blocked OS call cannot be forcibly stopped.
 */
function trackedInvoke<T>(
  command: string, args: Record<string, unknown>, options: CatalogRequestOptions = {},
): Promise<T> {
  if (options.signal?.aborted) {
    return Promise.reject(new Error("provider_cancelled: 已取消内容源请求"));
  }
  const requestId = ++nextCatalogRequestId;
  const timeoutMs = Math.min(30_000, Math.max(50, options.timeoutMs ?? 8_000));
  let interrupted = false;
  let rejectWait: (reason: Error) => void = () => {};
  const interruptedPromise = new Promise<never>((_resolve, reject) => {
    rejectWait = reject;
  });
  const cancel = (code: "provider_cancelled" | "provider_timeout") => {
    if (interrupted) return;
    interrupted = true;
    void desktopInvoke<boolean>("catalog_cancel_request", { requestId }).catch(() => {});
    rejectWait(new Error(code + ": " + (code === "provider_timeout"
      ? "内容源请求超时，请重试" : "内容源请求已取消")));
  };
  const onAbort = () => cancel("provider_cancelled");
  options.signal?.addEventListener("abort", onAbort, { once: true });
  const timer = setTimeout(() => cancel("provider_timeout"), timeoutMs);
  return Promise.race([
    desktopInvoke<T>(command, { ...args, requestId, timeoutMs }),
    interruptedPromise,
  ]).finally(() => {
    clearTimeout(timer);
    options.signal?.removeEventListener("abort", onAbort);
  });
}

export function isCatalogCancellation(reason: unknown): boolean {
  return String(reason).includes("provider_cancelled:");
}

export function catalogErrorMessage(reason: unknown): string {
  const value = String(reason);
  if (value.includes("provider_cancelled:")) return "已取消当前内容源请求";
  if (value.includes("provider_timeout:")) return "内容源请求超时，请重试";
  if (value.includes("provider_busy:")) return "当前内容请求过多，请稍后重试";
  if (value.includes("provider_not_found:")) return "该内容源不可用，请切换内容源";
  if (value.includes("hgcommunity_sdk_RATE_LIMITED")) return "社区 SDK 上游请求受限，请稍后再尝试，避免频繁刷新";
  if (value.includes("hgcommunity_sdk_ANONYMOUS_LIMITED")) return "社区 SDK 匿名访问受限，暂时无法获取该作品";
  if (value.includes("hgcommunity_sdk_AUTHENTICATION_REQUIRED")) return "该作品需要授权登录，当前实验模式不支持";
  if (value.includes("hgcommunity_sdk_CONTENT_UNAVAILABLE")) return "该作品暂不可访问，可能已下架或无访问权限";
  if (value.includes("hgcommunity_sdk_TRANSPORT")) return "社区 SDK 上游网络请求失败，请检查网络和代理配置";
  if (value.includes("hgcommunity_sdk_CHALLENGE_REQUIRED") ||
      value.includes("hgcommunity_sdk_SIGNATURE_REJECTED") ||
      value.includes("hgcommunity_sdk_DEVICE_REJECTED"))
    return "社区 SDK 访问受到上游验证限制，实验模式不会绕过";
  if (value.includes("hgcommunity_sdk_") || value.includes("hgcommunity_http_503"))
    return "社区 SDK 上游暂不可用（503），请稍后重试；未自动重复请求";
  if (value.includes("hgcommunity_timeout")) return "社区 SDK 请求超时，请检查网络，不要反复刷新";
  if (value.includes("series_not_found:")) return "该内容已不存在，请刷新列表";
  if (value.includes("invalid_query:")) return "搜索内容过长，请缩短关键词";
  if (value.includes("manifest_")) return value.replace(/^.*?(manifest_[a-z]+:)/, "").trim();
  if (value.includes("webdav_")) return value.replace(/^.*?(webdav_[a-z_]+:)/, "").trim();
  if (value.includes("http_")) return value.replace(/^.*?(http_[a-z_]+:)/, "").trim();
  return value;
}

export function searchCatalog(query: string): Promise<Series[]> {
  return desktopInvoke<Series[]>("catalog_search", { query });
}

export function importOwnedManifest(): Promise<number | null> {
  return desktopInvoke<number | null>("catalog_import_manifest");
}

export function clearOwnedManifest(): Promise<void> {
  return desktopInvoke<void>("catalog_clear_manifest");
}

export function subscribeRemoteManifest(url: string): Promise<number> {
  return desktopInvoke<number>("catalog_subscribe_manifest", { url });
}

export function refreshRemoteManifest(): Promise<number> {
  return desktopInvoke<number>("catalog_refresh_manifest");
}

export function getManifestSubscriptionInfo(): Promise<ManifestSubscriptionInfo> {
  return desktopInvoke<ManifestSubscriptionInfo>("catalog_manifest_info");
}

export function getWebDavStatus(): Promise<WebDavStatus> {
  return desktopInvoke<WebDavStatus>("webdav_get_status");
}

export function configureWebDav(
  name: string, baseUrl: string, username: string, password: string,
): Promise<WebDavStatus> {
  return desktopInvoke<WebDavStatus>("webdav_configure", { name, baseUrl, username, password });
}

export function syncWebDav(): Promise<WebDavStatus> {
  return desktopInvoke<WebDavStatus>("webdav_sync");
}

export function clearWebDav(): Promise<void> {
  return desktopInvoke<void>("webdav_clear");
}

export function listFavoriteSeries(): Promise<FavoriteSeries[]> {
  return desktopInvoke<FavoriteSeries[]>("catalog_favorites_list");
}

export function setFavoriteSeries(seriesId: string, favorite: boolean): Promise<FavoriteSeries[]> {
  return desktopInvoke<FavoriteSeries[]>("catalog_favorite_set", { seriesId, favorite });
}

export function listCatalogSources(): Promise<CatalogProvider[]> {
  return desktopInvoke<CatalogProvider[]>("catalog_sources");
}

export function searchCatalogPage(
  query: string, sourceId: string, offset = 0, limit = 24,
  options?: CatalogRequestOptions,
): Promise<CatalogSearchPage> {
  return trackedInvoke<CatalogSearchPage>("catalog_search_page", {
    query, sourceId, offset, limit,
  }, options);
}

export function getCatalogSeries(
  seriesId: string, options?: CatalogRequestOptions,
): Promise<SeriesDetail> {
  return trackedInvoke<SeriesDetail>("catalog_detail", { seriesId }, options);
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

export function getCatalogEpisodeProgress(seriesId: string): Promise<ProviderEpisodeProgress[]> {
  return desktopInvoke<ProviderEpisodeProgress[]>("catalog_episode_progress", { seriesId });
}

export function getRecentCatalogEpisodes(limit = 12): Promise<RecentCatalogEpisode[]> {
  return desktopInvoke<RecentCatalogEpisode[]>("catalog_recently_watched", { limit });
}
