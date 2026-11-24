import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  addLocalMedia, catalogErrorMessage, clearOwnedManifest, getCatalogEpisodeProgress,
  getCatalogSeries, importOwnedManifest, isCatalogCancellation, listCatalogSources,
  listFavoriteSeries, searchCatalogPage, setFavoriteSeries,
} from "./api";
import type { CatalogProvider, FavoriteSeries, ProviderEpisodeProgress, Series, SeriesDetail } from "./types";
import { formatEpisodePosition, latestEpisodeProgress } from "./progress";
import { watchedEpisodeCount } from "./recentActivity";
import { ContinueWatchingShelf } from "./ContinueWatchingShelf";
import { LocalLibraryView } from "./LocalLibraryView";
import { DiscoveryHome } from "./DiscoveryHome";
import { CoverImage } from "./CoverImage";
import { SourceManagerModal } from "./SourceManagerModal";
import { FavoritesView } from "./FavoritesView";
import { visibleCatalogProviders, visibleCatalogSeries } from "./discovery";
import {
  filterEpisodes, summarizeEpisodes, type EpisodeOrder, type EpisodeStatusFilter,
} from "./detailEpisodes";
import {
  accountErrorMessage,
  getAccountStatus,
  getCloudLibrary,
  logoutAccount,
  syncAccount,
  type AccountStatusSnapshot,
  type CloudLibrarySnapshot,
} from "../account/api";
import { AccountLoginModal } from "../account/AccountLoginModal";

interface Props {
  onPlay: (seriesId: string, episodeId: string, detail: SeriesDetail) => Promise<boolean>;
  onPlayLocal: (id: number, orderedIds: number[], origin: string, titles?: Record<string, string>) => Promise<boolean>;
  onOpenLocalFile: () => Promise<void>;
  playerError: string;
  onClearPlayerError: () => void;
  refreshSignal: number;
}

type NavMode = "home" | "browse" | "local" | "favorites" | "history";

export function CatalogView({
  onPlay, onPlayLocal, onOpenLocalFile, playerError, onClearPlayerError, refreshSignal,
}: Props) {
  const [mode, setMode] = useState<NavMode>("home");
  const [scrolled, setScrolled] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [input, setInput] = useState("");
  const [search, setSearch] = useState("");
  const [genreFilter, setGenreFilter] = useState("all");
  const [sourceId, setSourceId] = useState("all");
  const [sources, setSources] = useState<CatalogProvider[]>([]);
  const [refreshKey, setRefreshKey] = useState(0);

  // Browse grid state
  const [items, setItems] = useState<Series[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);

  // Detail state
  const [detail, setDetail] = useState<SeriesDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [episodeProgress, setEpisodeProgress] = useState<ProviderEpisodeProgress[]>([]);
  const [progressLoading, setProgressLoading] = useState(false);
  const [episodeQuery, setEpisodeQuery] = useState("");
  const [episodeFilter, setEpisodeFilter] = useState<EpisodeStatusFilter>("all");
  const [episodeOrder, setEpisodeOrder] = useState<EpisodeOrder>("ascending");
  const [descriptionExpanded, setDescriptionExpanded] = useState(false);

  // Misc state
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const [working, setWorking] = useState(false);
  const [manifestBusy, setManifestBusy] = useState(false);
  const [confirmManifestClear, setConfirmManifestClear] = useState(false);
  const [sourceModalOpen, setSourceModalOpen] = useState(false);
  const [sourceModalTab, setSourceModalTab] = useState<"webdav" | "remote">("webdav");
  const [favorites, setFavorites] = useState<FavoriteSeries[]>([]);

  // HG-5 Cloud Account state
  const [accountStatus, setAccountStatus] = useState<AccountStatusSnapshot | null>(null);
  const [cloudLibrary, setCloudLibrary] = useState<CloudLibrarySnapshot | null>(null);
  const [accountModalOpen, setAccountModalOpen] = useState(false);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const [accountSyncing, setAccountSyncing] = useState(false);

  const favoriteIds = useMemo(() => new Set(favorites.map(item => item.id)), [favorites]);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const requestVersion = useRef(0);
  const searchController = useRef<AbortController | null>(null);
  const pageController = useRef<AbortController | null>(null);
  const detailController = useRef<AbortController | null>(null);
  const recentPlayRef = useRef(false);

  const combinedRefresh = refreshKey + refreshSignal;
  const realSources = useMemo(() => visibleCatalogProviders(sources), [sources]);

  useEffect(() => {
    let alive = true;
    void listCatalogSources().then(result => {
      if (alive) setSources(result);
    }).catch(() => {});
    void listFavoriteSeries().then(result => {
      if (alive) setFavorites(result);
    }).catch(() => {});
    void getAccountStatus().then(status => {
      if (alive) setAccountStatus(status);
    }).catch(() => {});
    void getCloudLibrary().then(lib => {
      if (alive) setCloudLibrary(lib);
    }).catch(() => {});
    return () => { alive = false; };
  }, [combinedRefresh]);

  // On app startup, if a DPAPI-persisted HongGuo session exists, silently sync once.
  useEffect(() => {
    let alive = true;
    void getAccountStatus().then(async status => {
      if (!alive) return;
      setAccountStatus(status);
      if (status.signedIn && status.lastSyncedMs === null) {
        setAccountSyncing(true);
        try {
          const lib = await syncAccount();
          if (!alive) return;
          setCloudLibrary(lib);
          const refreshed = await getAccountStatus().catch(() => status);
          if (!alive) return;
          setAccountStatus(refreshed);
          setRefreshKey(v => v + 1);
        } catch {
          // Silent background sync on startup; user can manually retry from menu.
        } finally {
          if (alive) setAccountSyncing(false);
        }
      }
    }).catch(() => {});
    return () => { alive = false; };
  }, []);

  async function handleAccountSync() {
    if (accountSyncing) return;
    setAccountMenuOpen(false);
    setAccountSyncing(true);
    setError("");
    try {
      const lib = await syncAccount();
      setCloudLibrary(lib);
      const status = await getAccountStatus().catch(() => accountStatus);
      if (status) setAccountStatus(status);
      const resCount = lib.onlineReservations.length + lib.upcomingReservations.length;
      setToast(`云端同步完成：收藏 ${lib.favorites.length} 部 · 历史 ${lib.history.length} 部 · 预约 ${resCount} 部`);
      setRefreshKey(v => v + 1);
    } catch (reason) {
      setError(accountErrorMessage(reason));
    } finally {
      setAccountSyncing(false);
    }
  }

  async function handleAccountLogout() {
    if (accountSyncing) return;
    setAccountMenuOpen(false);
    try {
      const status = await logoutAccount();
      setAccountStatus(status);
      setCloudLibrary(null);
      setToast("已退出红果账号（本地片库与离线收藏已保留）");
      setRefreshKey(v => v + 1);
    } catch (reason) {
      setError(accountErrorMessage(reason));
    }
  }

  function handleSignedIn(
    status: AccountStatusSnapshot,
    cloud: CloudLibrarySnapshot | null,
    syncWarning?: string,
  ) {
    setAccountStatus(status);
    setCloudLibrary(cloud);
    if (syncWarning) {
      setToast(`已登录 ${status.nickname || "红果用户"}（首次同步失败，可点击手动同步）`);
    } else if (cloud) {
      const resCount = cloud.onlineReservations.length + cloud.upcomingReservations.length;
      setToast(`已登录 ${status.nickname || "红果用户"}：同步收藏 ${cloud.favorites.length} 部、历史 ${cloud.history.length} 部、预约 ${resCount} 部`);
    } else {
      setToast(`已登录 ${status.nickname || "红果用户"}`);
    }
    setRefreshKey(v => v + 1);
  }

  async function toggleFavoriteSeries(seriesId: string) {
    const next = !favoriteIds.has(seriesId);
    try {
      const updated = await setFavoriteSeries(seriesId, next);
      setFavorites(updated);
      setToast(next ? "已加入我的收藏" : "已从收藏移除");
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    }
  }

  useEffect(() => () => {
    searchController.current?.abort();
    pageController.current?.abort();
    detailController.current?.abort();
  }, []);

  // Auto-hide toast notice
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(""), 3800);
    return () => window.clearTimeout(timer);
  }, [toast]);

  // Browse query effect
  useEffect(() => {
    if (mode !== "browse") {
      searchController.current?.abort();
      return;
    }
    const version = ++requestVersion.current;
    const controller = new AbortController();
    searchController.current = controller;
    pageController.current?.abort();
    setLoadingMore(false);
    setLoading(true);
    setError("");
    setItems([]);
    setNextOffset(null);
    void searchCatalogPage(search, sourceId, 0, 36, { signal: controller.signal }).then(page => {
      if (version !== requestVersion.current || controller.signal.aborted) return;
      setItems(visibleCatalogSeries(page.items));
      setNextOffset(page.nextOffset);
    }).catch(reason => {
      if (version === requestVersion.current && !isCatalogCancellation(reason)) {
        setError(catalogErrorMessage(reason));
      }
    }).finally(() => {
      if (version === requestVersion.current) setLoading(false);
      if (searchController.current === controller) searchController.current = null;
    });
    return () => {
      controller.abort();
      if (version === requestVersion.current) requestVersion.current++;
    };
  }, [mode, search, sourceId, combinedRefresh]);

  // Episode progress for current detail
  useEffect(() => {
    let active = true;
    setEpisodeProgress([]);
    setProgressLoading(Boolean(detail));
    if (detail) {
      void getCatalogEpisodeProgress(detail.series.id).then(records => {
        if (active) setEpisodeProgress(records);
      }).catch(() => {}).finally(() => {
        if (active) setProgressLoading(false);
      });
    }
    return () => { active = false; };
  }, [detail, combinedRefresh]);

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    setScrolled(el.scrollTop > 28);
    if (
      mode === "browse" &&
      !detail &&
      nextOffset !== null &&
      !loading &&
      !loadingMore &&
      !working &&
      el.scrollHeight - el.scrollTop - el.clientHeight < 420
    ) {
      void loadMore();
    }
  }

  function navigateTo(next: NavMode) {
    detailController.current?.abort();
    detailController.current = null;
    setDetail(null);
    setDetailLoading(false);
    setMode(next);
    setError("");
    setMenuOpen(false);
    setAccountMenuOpen(false);
    onClearPlayerError();
    scrollRef.current?.scrollTo({ top: 0, behavior: "instant" });
  }

  function submitSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setDetail(null);
    setGenreFilter("all");
    setSourceId("all");
    setSearch(input.trim());
    setMode("browse");
    setMenuOpen(false);
  }

  async function showDetail(id: string) {
    detailController.current?.abort();
    const controller = new AbortController();
    detailController.current = controller;
    setError("");
    onClearPlayerError();
    setWorking(true);
    setDetailLoading(true);
    setMenuOpen(false);
    try {
      const result = await getCatalogSeries(id, { signal: controller.signal });
      if (!controller.signal.aborted) {
        setEpisodeQuery("");
        setEpisodeFilter("all");
        setEpisodeOrder("ascending");
        setDescriptionExpanded(false);
        setDetail(result);
        scrollRef.current?.scrollTo({ top: 0, behavior: "instant" });
      }
    } catch (reason) {
      if (!isCatalogCancellation(reason)) setError(catalogErrorMessage(reason));
    } finally {
      if (detailController.current === controller) {
        detailController.current = null;
        setWorking(false);
        setDetailLoading(false);
      }
    }
  }

  async function quickPlaySeries(series: Series) {
    if (working) return;
    setWorking(true);
    setError("");
    onClearPlayerError();
    try {
      const [current, progress] = await Promise.all([
        getCatalogSeries(series.id),
        getCatalogEpisodeProgress(series.id).catch(() => [] as ProviderEpisodeProgress[]),
      ]);
      const sorted = [...current.episodes].sort((a, b) => a.number - b.number);
      const last = latestEpisodeProgress(sorted, progress);
      const lastEp = last ? sorted.find(e => e.id === last.episodeId) : undefined;
      let target = lastEp && !last?.completed && lastEp.playable !== false ? lastEp : undefined;
      if (!target && lastEp && last?.completed) {
        const nextIdx = sorted.findIndex(e => e.id === lastEp.id) + 1;
        if (sorted[nextIdx] && sorted[nextIdx].playable !== false) target = sorted[nextIdx];
      }
      if (!target) {
        target = sorted.find(e => e.playable !== false);
      }
      if (target) {
        await onPlay(current.series.id, target.id, current);
      } else {
        setDetail(current);
      }
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setWorking(false);
    }
  }

  async function loadMore() {
    if (nextOffset === null || loadingMore || loading || working) return;
    const version = requestVersion.current;
    const controller = new AbortController();
    pageController.current = controller;
    setLoadingMore(true);
    setError("");
    try {
      const page = await searchCatalogPage(search, sourceId, nextOffset, 36, {
        signal: controller.signal,
      });
      if (version !== requestVersion.current || controller.signal.aborted) return;
      setItems(previous => {
        const seen = new Set(previous.map(item => item.id));
        const fresh = visibleCatalogSeries(page.items).filter(item => !seen.has(item.id));
        return [...previous, ...fresh];
      });
      setNextOffset(page.nextOffset);
    } catch (reason) {
      if (version === requestVersion.current && !isCatalogCancellation(reason)) {
        setError(catalogErrorMessage(reason));
      }
    } finally {
      if (pageController.current === controller) pageController.current = null;
      if (version === requestVersion.current) setLoadingMore(false);
    }
  }

  async function handleAddLocal() {
    if (working) return;
    setMenuOpen(false);
    setWorking(true);
    setError("");
    try {
      const added = await addLocalMedia();
      if (added) {
        setToast(`已添加「${added.title}」到本地片库`);
        setRefreshKey(v => v + 1);
      }
    } catch {
      setError("添加本地视频失败，请确认文件为 MP4 格式。");
    } finally {
      setWorking(false);
    }
  }

  async function handleImportManifest() {
    if (manifestBusy) return;
    setMenuOpen(false);
    setManifestBusy(true);
    setError("");
    try {
      const count = await importOwnedManifest();
      if (count !== null) {
        setConfirmManifestClear(false);
        setToast(`成功导入 ${count} 部剧集`);
        setRefreshKey(v => v + 1);
      }
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setManifestBusy(false);
    }
  }

  async function handleClearManifest() {
    if (!confirmManifestClear) {
      setConfirmManifestClear(true);
      return;
    }
    setManifestBusy(true);
    setError("");
    try {
      await clearOwnedManifest();
      setConfirmManifestClear(false);
      setMenuOpen(false);
      setToast("已清空导入的剧集清单");
      setRefreshKey(v => v + 1);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setManifestBusy(false);
    }
  }

  async function playEpisode(seriesId: string, episodeId: string) {
    if (!detail || detail.series.id !== seriesId) return;
    setWorking(true);
    try {
      await onPlay(seriesId, episodeId, detail);
    } finally {
      setWorking(false);
    }
  }

  async function playRecentProvider(seriesId: string, episodeId: string): Promise<boolean> {
    if (recentPlayRef.current || working) return false;
    recentPlayRef.current = true;
    setWorking(true);
    setError("");
    try {
      const current = await getCatalogSeries(seriesId);
      if (current.series.id !== seriesId || !current.episodes.some(item => item.id === episodeId)) {
        throw new Error("该剧集已变更，请重新进入详情");
      }
      return await onPlay(seriesId, episodeId, current);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
      return false;
    } finally {
      recentPlayRef.current = false;
      setWorking(false);
    }
  }

  async function playRecentLocal(id: number, title?: string): Promise<boolean> {
    if (recentPlayRef.current || working) return false;
    recentPlayRef.current = true;
    setWorking(true);
    setError("");
    try {
      const titles = title ? { [String(id)]: title } : undefined;
      return await onPlayLocal(id, [id], "继续观看", titles);
    } catch (reason) {
      setError(String(reason));
      return false;
    } finally {
      recentPlayRef.current = false;
      setWorking(false);
    }
  }

  // Detail derived state
  const sortedEpisodes = detail ? [...detail.episodes].sort((a, b) => a.number - b.number) : [];
  const lastProgress = detail ? latestEpisodeProgress(detail.episodes, episodeProgress) : null;
  const lastEpisode = lastProgress && detail?.episodes.find(item => item.id === lastProgress.episodeId);
  const nextEpisode = lastEpisode && lastProgress?.completed
    ? sortedEpisodes[sortedEpisodes.findIndex(item => item.id === lastEpisode.id) + 1]
    : undefined;
  const firstEpisode = sortedEpisodes[0];
  const playedEpisodes = detail ? watchedEpisodeCount(detail.episodes.map(e => e.id), episodeProgress) : 0;
  const episodeRows = detail
    ? filterEpisodes(detail.episodes, episodeProgress, detail.series.id, episodeQuery, episodeFilter, episodeOrder)
    : [];
  const episodeSummary = detail
    ? summarizeEpisodes(detail.episodes, episodeProgress, detail.series.id)
    : null;
  const descriptionCanExpand = Boolean(detail && detail.series.description.trim().length > 180);
  const providerCanPlay = detail
    ? (sources.find(p => p.id === detail.series.providerId)?.supportsPlay ?? true)
    : false;

  const browseGenres = Array.from(new Set(items.map(i => i.genre.trim()).filter(Boolean)))
    .sort((a, b) => a.localeCompare(b, "zh-CN"));
  const visibleBrowseItems = genreFilter === "all"
    ? items
    : items.filter(i => i.genre.trim() === genreFilter);

  const hasHeroOverlap = (mode === "home" && !detail) || Boolean(detail);

  return (
    <div className="nf-shell">
      {/* Netflix Floating Top Navbar */}
      <header className={"nf-navbar" + (scrolled || !hasHeroOverlap ? " is-solid" : "")}>
        <div className="nf-nav-left">
          <button
            type="button"
            className="nf-brand"
            onClick={() => navigateTo("home")}
            aria-label="HongGuo 首页"
          >
            HONGGUO
          </button>
          <nav className="nf-nav-links" aria-label="主导航">
            <button type="button" className={!detail && mode === "home" ? "is-active" : ""}
              onClick={() => navigateTo("home")}>首页</button>
            <button type="button" className={!detail && mode === "browse" ? "is-active" : ""}
              onClick={() => { setSourceId("all"); setGenreFilter("all"); setSearch(""); setInput(""); navigateTo("browse"); }}>
              片库
            </button>
            <button type="button" className={!detail && mode === "local" ? "is-active" : ""}
              onClick={() => navigateTo("local")}>本地视频</button>
            <button type="button" className={!detail && mode === "favorites" ? "is-active" : ""}
              onClick={() => navigateTo("favorites")}>我的收藏</button>
            <button type="button" className={!detail && mode === "history" ? "is-active" : ""}
              onClick={() => navigateTo("history")}>观看历史</button>
          </nav>
        </div>

        <div className="nf-nav-right">
          <form
            className={"nf-nav-search" + (searchOpen || input ? " is-open" : "")}
            onSubmit={submitSearch}
          >
            <button
              type="button"
              className="nf-nav-icon-btn"
              aria-label="搜索剧集"
              onClick={() => {
                setSearchOpen(v => !v);
                setTimeout(() => searchInputRef.current?.focus(), 60);
              }}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <circle cx="11" cy="11" r="7" fill="none" stroke="currentColor" strokeWidth="2.1" />
                <path d="m20 20-3.8-3.8" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" />
              </svg>
            </button>
            <input
              ref={searchInputRef}
              type="search"
              value={input}
              maxLength={128}
              placeholder="搜索剧名、题材…"
              aria-label="搜索剧集"
              onChange={e => setInput(e.target.value)}
              onBlur={() => { if (!input.trim()) setSearchOpen(false); }}
            />
          </form>

          <button
            type="button"
            className="nf-nav-action-btn"
            disabled={working}
            onClick={() => void onOpenLocalFile()}
            title="选择电脑上的 MP4 文件立即播放"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
            打开视频
          </button>

          {/* HG-5 Cloud Account Login / Status Menu */}
          <div className="nf-nav-menu-anchor">
            {accountStatus?.signedIn ? (
              <>
                <button
                  type="button"
                  className={"nf-account-pill" + (accountMenuOpen ? " is-active" : "")}
                  aria-expanded={accountMenuOpen}
                  title="红果账号与云端片单同步"
                  onClick={() => {
                    setAccountMenuOpen(v => !v);
                    setMenuOpen(false);
                  }}
                >
                  <span className={"nf-account-dot is-online" + (accountSyncing ? " is-syncing" : "")} aria-hidden="true" />
                  <span className="nf-account-name">{accountStatus.nickname || "红果用户"}</span>
                </button>
                {accountMenuOpen && (
                  <div className="nf-dropdown nf-account-dropdown" role="menu">
                    <div className="nf-account-dropdown-head">
                      <strong>{accountStatus.nickname || "红果用户"}</strong>
                      <span>
                        收藏 {accountStatus.favoritesCount} · 历史 {accountStatus.historyCount} · 预约 {accountStatus.reservationsCount}
                      </span>
                    </div>
                    <div className="nf-dropdown-sep" />
                    <button
                      type="button"
                      role="menuitem"
                      disabled={accountSyncing}
                      onClick={() => void handleAccountSync()}
                    >
                      {accountSyncing ? "正在同步云端片单…" : "立即同步云端片单"}
                    </button>
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => navigateTo("favorites")}
                    >
                      查看收藏与 App 预约
                    </button>
                    <div className="nf-dropdown-sep" />
                    <button
                      type="button"
                      role="menuitem"
                      className="is-danger"
                      disabled={accountSyncing}
                      onClick={() => void handleAccountLogout()}
                    >
                      退出登录
                    </button>
                  </div>
                )}
              </>
            ) : (
              <button
                type="button"
                className="nf-account-btn"
                onClick={() => {
                  setMenuOpen(false);
                  setAccountMenuOpen(false);
                  setAccountModalOpen(true);
                }}
                title="使用手机红果 App 扫码登录并同步片单"
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="M3 3h7v7H3V3zm2 2v3h3V5H5zm9-2h7v7h-7V3zm2 2v3h3V5h-3zM3 14h7v7H3v-7zm2 2v3h3v-3H5zm10 0h2v2h-2v-2zm3-2h3v3h-3v-3zm-3 5h3v2h-3v-2zm4-1h2v3h-2v-3z"
                    fill="currentColor"
                  />
                </svg>
                扫码登录
              </button>
            )}
          </div>

          <div className="nf-nav-menu-anchor">
            <button
              type="button"
              className={"nf-nav-icon-btn" + (menuOpen ? " is-active" : "")}
              aria-label="添加与管理媒体"
              aria-expanded={menuOpen}
              title="添加与管理媒体"
              onClick={() => { setMenuOpen(v => !v); setAccountMenuOpen(false); setConfirmManifestClear(false); }}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
              </svg>
            </button>
            {menuOpen && (
              <div className="nf-dropdown" role="menu">
                <button type="button" role="menuitem" disabled={working}
                  onClick={() => { setMenuOpen(false); setSourceModalTab("webdav"); setSourceModalOpen(true); }}>
                  接入 WebDAV（Alist / NAS）
                </button>
                <button type="button" role="menuitem" disabled={working}
                  onClick={() => { setMenuOpen(false); setSourceModalTab("remote"); setSourceModalOpen(true); }}>
                  远程订阅清单 (HTTPS JSON)
                </button>
                <div className="nf-dropdown-sep" />
                <button type="button" role="menuitem" disabled={working}
                  onClick={() => { setMenuOpen(false); void onOpenLocalFile(); }}>
                  直接播放本地 MP4
                </button>
                <button type="button" role="menuitem" disabled={working}
                  onClick={() => void handleAddLocal()}>
                  添加 MP4 到本地片库
                </button>
                <button type="button" role="menuitem" disabled={manifestBusy}
                  onClick={() => void handleImportManifest()}>
                  {manifestBusy ? "导入中…" : "导入本地清单 (.json)"}
                </button>
                <button type="button" role="menuitem" className="is-danger" disabled={manifestBusy}
                  onClick={() => void handleClearManifest()}>
                  {confirmManifestClear ? "再次点击确认清空清单" : "清空已导入清单"}
                </button>
              </div>
            )}
          </div>
        </div>
      </header>

      {/* Scrollable Main Canvas */}
      <div className="nf-viewport" ref={scrollRef} onScroll={onScroll}
        onClick={() => {
          if (menuOpen) setMenuOpen(false);
          if (accountMenuOpen) setAccountMenuOpen(false);
        }}>

        {toast && <div className="nf-toast" role="status">{toast}</div>}

        {(error || playerError) && (
          <div className="nf-global-alert" role="alert">
            <span>{error || playerError}</span>
            <button type="button" onClick={() => { setError(""); onClearPlayerError(); }}>关闭</button>
          </div>
        )}

        {detailLoading && !detail && (
          <div className="nf-page-loading" role="status">
            <span className="nf-red-loader" />
          </div>
        )}

        {/* Netflix Series Detail View */}
        {detail ? (
          <article className="nf-detail">
            <div className="nf-detail-hero">
              <div className={"nf-detail-backdrop artwork-" + detail.series.artwork} aria-hidden="true">
                <CoverImage url={detail.series.coverUrl} seriesId={detail.series.id} eager priority />
              </div>
              <div className="nf-detail-vignette" aria-hidden="true" />
              <div className="nf-detail-hero-content">
                <button type="button" className="nf-back-pill" disabled={working}
                  onClick={() => setDetail(null)}>
                  ← 返回
                </button>
                <div className="nf-detail-columns">
                  <div className={"nf-detail-poster artwork-" + detail.series.artwork} aria-hidden="true">
                    <span className="nf-poster-fallback-title">{detail.series.title}</span>
                    <CoverImage url={detail.series.coverUrl} seriesId={detail.series.id} eager />
                  </div>
                  <div className="nf-detail-info">
                    <div className="nf-detail-tags">
                      {detail.series.genre && <span>{detail.series.genre}</span>}
                      <span>共 {detail.episodes.length} 集</span>
                      {detail.series.providerId !== "local" && playedEpisodes > 0 && (
                        <span>已看 {playedEpisodes} 集</span>
                      )}
                      {detail.series.badge && <span>{detail.series.badge}</span>}
                    </div>
                    <h1>{detail.series.title}</h1>
                    <p className={"nf-detail-desc" + (descriptionCanExpand && !descriptionExpanded ? " is-clamped" : "")}>
                      {detail.series.description || "暂无简介"}
                    </p>
                    {descriptionCanExpand && (
                      <button type="button" className="nf-link-btn"
                        aria-expanded={descriptionExpanded}
                        onClick={() => setDescriptionExpanded(v => !v)}>
                        {descriptionExpanded ? "收起简介" : "展开完整简介"}
                      </button>
                    )}

                    <div className="nf-detail-cta-row">
                      {lastEpisode && lastProgress ? (
                        <button
                          type="button"
                          className="nf-btn-play"
                          disabled={working || !providerCanPlay || lastEpisode.playable === false}
                          onClick={() => void playEpisode(detail.series.id, lastEpisode.id)}
                        >
                          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12.5-7.5z" fill="currentColor" /></svg>
                          {lastProgress.completed
                            ? `重看第 ${lastEpisode.number} 集`
                            : `继续观看 · 第 ${lastEpisode.number} 集 (${formatEpisodePosition(lastProgress.positionSec)})`}
                        </button>
                      ) : firstEpisode ? (
                        <button
                          type="button"
                          className="nf-btn-play"
                          disabled={working || !providerCanPlay || firstEpisode.playable === false}
                          onClick={() => void playEpisode(detail.series.id, firstEpisode.id)}
                        >
                          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12.5-7.5z" fill="currentColor" /></svg>
                          {!providerCanPlay ? "仅浏览目录"
                            : firstEpisode.playable === false ? "首集暂不可播"
                              : `开始观看 · 第 ${firstEpisode.number} 集`}
                        </button>
                      ) : null}

                      {nextEpisode && nextEpisode.playable !== false && providerCanPlay && (
                        <button
                          type="button"
                          className="nf-btn-secondary"
                          disabled={working}
                          onClick={() => void playEpisode(detail.series.id, nextEpisode.id)}
                        >
                          播放下一集（第 {nextEpisode.number} 集）
                        </button>
                      )}

                      <button
                        type="button"
                        className={"nf-btn-secondary nf-btn-fav" + (favoriteIds.has(detail.series.id) ? " is-favorited" : "")}
                        disabled={working}
                        aria-pressed={favoriteIds.has(detail.series.id)}
                        onClick={() => void toggleFavoriteSeries(detail.series.id)}
                      >
                        {favoriteIds.has(detail.series.id) ? "★ 已收藏" : "☆ 加入收藏"}
                      </button>
                    </div>

                    {detail.series.providerId !== "local" && episodeSummary && episodeSummary.total > 0 && (
                      <div className="nf-detail-progress-bar">
                        <div className="nf-detail-progress-head">
                          <span>观看进度</span>
                          <strong>
                            {progressLoading ? "同步中…" : `已看完 ${episodeSummary.completed} / ${episodeSummary.total} 集`}
                          </strong>
                        </div>
                        <div className="nf-progress-track" role="progressbar"
                          aria-valuemin={0} aria-valuemax={100} aria-valuenow={episodeSummary.percent}>
                          <span style={{ width: `${episodeSummary.percent}%` }} />
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            </div>

            {/* Episodes Grid Section */}
            <section className="nf-episodes-section" aria-labelledby="nf-episodes-heading">
              <div className="nf-episodes-toolbar">
                <div className="nf-episodes-title-group">
                  <h2 id="nf-episodes-heading">剧集</h2>
                  <span>{episodeRows.length} / {detail.episodes.length} 集</span>
                </div>
                <div className="nf-episodes-controls">
                  <input
                    type="search"
                    className="nf-input-dark"
                    aria-label="搜索剧集集数或标题"
                    placeholder="搜索集数或标题…"
                    value={episodeQuery}
                    onChange={e => setEpisodeQuery(e.target.value)}
                  />
                  <select
                    className="nf-select-dark"
                    aria-label="按观看状态筛选剧集"
                    value={episodeFilter}
                    onChange={e => setEpisodeFilter(e.target.value as EpisodeStatusFilter)}
                  >
                    <option value="all">全部剧集</option>
                    <option value="incomplete">未完播</option>
                    <option value="completed">已看完</option>
                    <option value="unavailable">暂不可播</option>
                  </select>
                  <button
                    type="button"
                    className="nf-btn-ghost"
                    aria-pressed={episodeOrder === "descending"}
                    onClick={() => setEpisodeOrder(v => (v === "ascending" ? "descending" : "ascending"))}
                  >
                    {episodeOrder === "ascending" ? "正序 ↑" : "倒序 ↓"}
                  </button>
                </div>
              </div>

              {episodeRows.length > 0 ? (
                <div className="nf-episodes-grid">
                  {episodeRows.map(row => {
                    const { episode, progress: saved, percent } = row;
                    const isResume = lastEpisode?.id === episode.id;
                    const disabled = working || !providerCanPlay || episode.playable === false;
                    const statusText = episode.playable === false
                      ? "暂不可播"
                      : !providerCanPlay
                        ? "仅浏览"
                        : saved?.completed
                          ? "已看完"
                          : saved
                            ? `看到 ${formatEpisodePosition(saved.positionSec)}`
                            : (episode.runtimeLabel && episode.runtimeLabel !== "—" ? episode.runtimeLabel : "");
                    return (
                      <button
                        key={episode.id}
                        type="button"
                        disabled={disabled}
                        className={
                          "nf-ep-card"
                          + (episode.playable === false ? " is-unavailable" : "")
                          + (saved?.completed ? " is-complete" : "")
                          + (isResume ? " is-resume" : "")
                        }
                        onClick={() => void playEpisode(detail.series.id, episode.id)}
                      >
                        <div className="nf-ep-card-top">
                          <span className="nf-ep-num">{String(episode.number).padStart(2, "0")}</span>
                          <span className="nf-ep-status">{statusText}</span>
                        </div>
                        <strong title={episode.title}>{episode.title}</strong>
                        {percent !== null && (
                          <span className="nf-ep-bar" role="progressbar"
                            aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
                            <i style={{ width: `${percent}%` }} />
                          </span>
                        )}
                      </button>
                    );
                  })}
                </div>
              ) : (
                <div className="nf-empty-panel">
                  <p>{detail.episodes.length === 0 ? "该作品暂未提供剧集。" : "没有符合筛选条件的剧集。"}</p>
                  {(episodeQuery || episodeFilter !== "all") && (
                    <button type="button" className="nf-btn-secondary"
                      onClick={() => { setEpisodeQuery(""); setEpisodeFilter("all"); }}>
                      清除筛选
                    </button>
                  )}
                </div>
              )}
            </section>
          </article>
        ) : mode === "home" ? (
          <DiscoveryHome
            sources={sources}
            busy={working}
            refreshSignal={combinedRefresh}
            favoriteIds={favoriteIds}
            onToggleFavorite={toggleFavoriteSeries}
            onSelect={id => void showDetail(id)}
            onQuickPlay={series => void quickPlaySeries(series)}
            onExplore={(src, q) => {
              setSourceId(src);
              setInput(q);
              setSearch(q);
              setGenreFilter("all");
              setMode("browse");
            }}
            onPlayProvider={playRecentProvider}
            onPlayLocal={playRecentLocal}
            onOpenLocalFile={onOpenLocalFile}
            onAddLocalMedia={handleAddLocal}
            onImportManifest={() => {
              setSourceModalTab("webdav");
              setSourceModalOpen(true);
              return Promise.resolve();
            }}
          />
        ) : mode === "favorites" ? (
          <FavoritesView
            favorites={favorites}
            busy={working}
            refreshSignal={combinedRefresh}
            accountStatus={accountStatus}
            cloudLibrary={cloudLibrary}
            accountSyncing={accountSyncing}
            onOpenLogin={() => setAccountModalOpen(true)}
            onSyncAccount={handleAccountSync}
            onSelectSeries={id => void showDetail(id)}
            onQuickPlaySeries={series => void quickPlaySeries(series)}
            onRemoveFavorite={id => toggleFavoriteSeries(id)}
            onPlayLocal={onPlayLocal}
            onGoHome={() => navigateTo("home")}
          />
        ) : mode === "local" ? (
          <div className="nf-page-container">
            <div className="nf-page-header">
              <h1>本地视频</h1>
            </div>
            <LocalLibraryView
              key="local"
              refreshSignal={combinedRefresh}
              onPlay={onPlayLocal}
            />
          </div>
        ) : mode === "history" ? (
          <div className="nf-page-container">
            <div className="nf-page-header">
              <h1>观看历史</h1>
              {accountStatus?.signedIn ? (
                <button
                  type="button"
                  className="nf-btn-secondary"
                  disabled={accountSyncing}
                  onClick={() => void handleAccountSync()}
                >
                  {accountSyncing ? "正在同步…" : "同步云端记录"}
                </button>
              ) : (
                <button
                  type="button"
                  className="nf-btn-secondary"
                  onClick={() => setAccountModalOpen(true)}
                >
                  扫码同步 App 历史
                </button>
              )}
            </div>
            <ContinueWatchingShelf
              includeLocal={false}
              limit={24}
              busy={working}
              refreshKey={combinedRefresh}
              onPlayProvider={playRecentProvider}
              onPlayLocal={playRecentLocal}
            />
            <div className="nf-section-divider">
              <h2>本地视频记录</h2>
            </div>
            <LocalLibraryView
              key="history"
              historyOnly
              refreshSignal={combinedRefresh}
              onPlay={onPlayLocal}
            />
          </div>
        ) : (
          /* Browse All Grid */
          <div className="nf-page-container">
            <div className="nf-page-header nf-browse-header">
              <div>
                <h1>{search ? `搜索：“${search}”` : "片库探索"}</h1>
                {search && (
                  <button type="button" className="nf-link-btn"
                    onClick={() => { setInput(""); setSearch(""); }}>
                    清除搜索
                  </button>
                )}
              </div>
              <div className="nf-browse-tools">
                {realSources.length > 1 && (
                  <div className="nf-chip-group">
                    <button type="button" className={sourceId === "all" ? "is-active" : ""}
                      onClick={() => { setSourceId("all"); setGenreFilter("all"); }}>全部</button>
                    {realSources.map(p => (
                      <button key={p.id} type="button" className={sourceId === p.id ? "is-active" : ""}
                        onClick={() => { setSourceId(p.id); setGenreFilter("all"); }}>{p.name}</button>
                    ))}
                  </div>
                )}
              </div>
            </div>

            {browseGenres.length > 1 && (
              <div className="nf-filter-strip nf-browse-genres">
                <div className="nf-chip-group">
                  <button type="button" className={genreFilter === "all" ? "is-active" : ""}
                    onClick={() => setGenreFilter("all")}>全部题材</button>
                  {browseGenres.map(name => (
                    <button key={name} type="button" className={genreFilter === name ? "is-active" : ""}
                      onClick={() => setGenreFilter(name)}>{name}</button>
                  ))}
                </div>
              </div>
            )}

            {loading ? (
              <div className="nf-poster-grid-skeleton" role="status">
                {Array.from({ length: 10 }).map((_, i) => <span key={i} />)}
              </div>
            ) : visibleBrowseItems.length === 0 ? (
              <div className="nf-empty-panel">
                <h3>{search ? "未找到匹配的作品" : "片库暂无内容"}</h3>
                <p>{search ? "换个关键词试试，或浏览全部内容。" : "你可以添加本地 MP4 视频，或导入剧集 JSON 清单。"}</p>
                <div className="nf-empty-actions">
                  <button type="button" className="nf-btn-play" onClick={() => void handleAddLocal()}>
                    ＋ 添加本地 MP4
                  </button>
                  <button type="button" className="nf-btn-secondary" onClick={() => void handleImportManifest()}>
                    导入剧集清单
                  </button>
                </div>
              </div>
            ) : (
              <>
                <div className="nf-poster-grid">
                  {visibleBrowseItems.map((item, idx) => {
                    const isFav = favoriteIds.has(item.id);
                    return (
                      <div key={item.id} className="nf-poster-card-wrap">
                        <button
                          type="button"
                          className="nf-poster-card"
                          disabled={working}
                          onClick={() => void showDetail(item.id)}
                        >
                          <span className={"nf-poster-frame artwork-" + item.artwork} aria-hidden="true">
                            <span className="nf-poster-fallback-title">{item.title}</span>
                            <CoverImage url={item.coverUrl} seriesId={item.id} eager={idx < 10} priority={idx < 3} />
                            <span className="nf-poster-gradient" />
                            <span className="nf-poster-badge">
                              {item.episodeCount > 1 ? `全 ${item.episodeCount} 集` : "影片"}
                            </span>
                            <span className="nf-poster-hover-play">
                              <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                            </span>
                          </span>
                          <span className="nf-poster-info">
                            <strong title={item.title}>{item.title}</strong>
                            <small>{item.genre || item.badge || "本地片库"}</small>
                          </span>
                        </button>
                        <button
                          type="button"
                          className={"nf-poster-fav-btn" + (isFav ? " is-favorited" : "")}
                          disabled={working}
                          aria-pressed={isFav}
                          aria-label={(isFav ? "取消收藏 " : "收藏 ") + item.title}
                          title={isFav ? "取消收藏" : "加入我的收藏"}
                          onClick={e => {
                            e.stopPropagation();
                            void toggleFavoriteSeries(item.id);
                          }}
                        >
                          {isFav ? "★" : "☆"}
                        </button>
                      </div>
                    );
                  })}
                </div>
                {nextOffset !== null && (
                  <div className="nf-home-footer-actions">
                    <button type="button" className="nf-btn-secondary" disabled={loadingMore || working}
                      onClick={() => void loadMore()}>
                      {loadingMore ? "正在加载…" : "加载更多"}
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        )}
      </div>
      <SourceManagerModal
        open={sourceModalOpen}
        initialTab={sourceModalTab}
        onClose={() => setSourceModalOpen(false)}
        onChanged={msg => {
          setToast(msg);
          setRefreshKey(v => v + 1);
        }}
      />
      <AccountLoginModal
        open={accountModalOpen}
        onClose={() => setAccountModalOpen(false)}
        onSignedIn={handleSignedIn}
      />
    </div>
  );
}
