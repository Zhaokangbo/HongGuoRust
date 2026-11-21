import type { CatalogProvider, CatalogSearchPage, LocalMedia, Series, SeriesDetail } from "./types";

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
  if (value.includes("series_not_found:")) return "该内容已不存在，请刷新列表";
  if (value.includes("invalid_query:")) return "搜索内容过长，请缩短关键词";
  return value;
}

export function searchCatalog(query: string): Promise<Series[]> {
  return desktopInvoke<Series[]>("catalog_search", { query });
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
