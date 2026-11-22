import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  catalogErrorMessage, clearOwnedManifest, getCatalogSeries, getCatalogEpisodeProgress, importOwnedManifest,
  isCatalogCancellation, listCatalogSources, searchCatalogPage,
} from "./api";
import type { CatalogProvider, ProviderEpisodeProgress, Series, SeriesDetail } from "./types";
import { latestEpisodeProgress, formatEpisodePosition } from "./progress";
import { watchedEpisodeCount, watchedPercent } from "./recentActivity";
import { ContinueWatchingShelf } from "./ContinueWatchingShelf";
import { LocalLibraryView } from "./LocalLibraryView";
import { DiscoveryHome } from "./DiscoveryHome";
import { CoverImage } from "./CoverImage";

interface Props {
  onBack: () => Promise<void>;
  onPlay: (seriesId: string, episodeId: string, detail: SeriesDetail) => Promise<boolean>;
  onPlayLocal: (id: number, orderedIds: number[], origin: string) => Promise<boolean>;
  playerError: string;
}

export function CatalogView({ onBack, onPlay, onPlayLocal, playerError }: Props) {
  const [mode, setMode] = useState<"home" | "demo" | "local" | "history" | "favorites">("home");
  const [input, setInput] = useState("");
  const [search, setSearch] = useState("");
  const [genreFilter, setGenreFilter] = useState("all");
  const [sourceId, setSourceId] = useState("demo");
  const [sources, setSources] = useState<CatalogProvider[]>([]);
  const [refreshKey, setRefreshKey] = useState(0);
  const [items, setItems] = useState<Series[]>([]);
  const [total, setTotal] = useState(0);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  const requestVersion = useRef(0);
  const searchController = useRef<AbortController | null>(null);
  const pageController = useRef<AbortController | null>(null);
  const detailController = useRef<AbortController | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detail, setDetail] = useState<SeriesDetail | null>(null);
  const [episodeProgress, setEpisodeProgress] = useState<ProviderEpisodeProgress[]>([]);
  const [progressLoading, setProgressLoading] = useState(false);
  const recentPlayRef = useRef(false);
  const [progressError, setProgressError] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);
  const [manifestBusy, setManifestBusy] = useState(false);
  const [manifestInfo, setManifestInfo] = useState("");
  const [confirmManifestClear, setConfirmManifestClear] = useState(false);

  // Leaving the unified catalog must invalidate any still-loading detail.
  useEffect(() => {
    if (mode !== "demo") {
      detailController.current?.abort();
      detailController.current = null;
      setDetail(null);
      setDetailLoading(false);
      setWorking(false);
    }
  }, [mode]);

  // The search effect separately cleans its own request on dependency change.
  useEffect(() => () => {
    pageController.current?.abort();
    detailController.current?.abort();
  }, []);

  useEffect(() => {
    let alive = true;
    void listCatalogSources().then(result => {
      if (alive) setSources(result);
    }).catch(reason => {
      if (alive) setError(String(reason));
    });
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    if (mode !== "demo") {
      searchController.current?.abort();
      return;
    }
    const version = ++requestVersion.current;
    const controller = new AbortController();
    searchController.current = controller;
    pageController.current?.abort();
    detailController.current?.abort();
    setLoadingMore(false);
    setDetailLoading(false);
    setLoading(true);
    setError("");
    setDetail(null);
    setItems([]);
    setTotal(0);
    setNextOffset(null);
    void searchCatalogPage(search, sourceId, 0, 24, { signal: controller.signal }).then(page => {
      if (version !== requestVersion.current || controller.signal.aborted) return;
      setItems(page.items);
      setTotal(page.total);
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
  }, [mode, search, sourceId, refreshKey]);

  // Provider history is read-only here. Keep stale responses from attaching to a new detail.
  useEffect(() => {
    let active = true;
    setEpisodeProgress([]);
    setProgressError("");
    setProgressLoading(Boolean(detail));
    if (detail) {
      void getCatalogEpisodeProgress(detail.series.id).then(records => {
        if (active) setEpisodeProgress(records);
      }).catch(reason => {
        if (active) setProgressError(String(reason));
      }).finally(() => {
        if (active) setProgressLoading(false);
      });
    }
    return () => { active = false; };
  }, [detail, refreshKey]);

  async function loadMore() {
    if (nextOffset === null || loadingMore || loading || working) return;
    const version = requestVersion.current;
    const controller = new AbortController();
    pageController.current = controller;
    setLoadingMore(true);
    setError("");
    try {
      const page = await searchCatalogPage(search, sourceId, nextOffset, 24, {
        signal: controller.signal,
      });
      if (version !== requestVersion.current || controller.signal.aborted) return;
      setItems(previous => [...previous, ...page.items]);
      setTotal(page.total);
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

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setGenreFilter("all");
    setDetail(null);
    setSearch(input.trim());
  }

  async function showDetail(id: string) {
    detailController.current?.abort();
    const controller = new AbortController();
    detailController.current = controller;
    setError("");
    setWorking(true);
    setDetailLoading(true);
    try {
      const result = await getCatalogSeries(id, { signal: controller.signal });
      if (!controller.signal.aborted) setDetail(result);
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

  function cancelPending() {
    if (!(loading || loadingMore || detailLoading)) return;
    requestVersion.current++;
    searchController.current?.abort();
    pageController.current?.abort();
    detailController.current?.abort();
    setLoading(false);
    setLoadingMore(false);
    setDetailLoading(false);
    setWorking(false);
    setError("已取消内容源请求，点击“刷新内容”可重新加载");
  }

  function refreshCatalog() {
    setRefreshKey(previous => previous + 1);
  }

  function openExplorer(fromSource: string, fromQuery: string) {
    setSourceId(fromSource);
    setInput(fromQuery);
    setSearch(fromQuery);
    setGenreFilter("all");
    setError("");
    setDetail(null);
    setMode("demo");
  }

  function selectSource(id: string) {
    setMode("demo");
    setSourceId(id);
    setInput("");
    setSearch("");
    setGenreFilter("all");
    setError("");
    setConfirmManifestClear(false);
    setManifestInfo("");
  }

  async function importManifest() {
    if (manifestBusy) return;
    setManifestBusy(true);
    setError("");
    setManifestInfo("");
    try {
      const count = await importOwnedManifest();
      if (count === null) return;
      setSourceId("manifest");
      setInput("");
      setSearch("");
      setConfirmManifestClear(false);
      setManifestInfo(`已导入 ${count} 部作品。清单保存在本机应用数据目录。`);
      refreshCatalog();
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setManifestBusy(false);
    }
  }

  async function clearManifest() {
    if (!confirmManifestClear) {
      setConfirmManifestClear(true);
      return;
    }
    setManifestBusy(true);
    setError("");
    try {
      await clearOwnedManifest();
      setManifestInfo("已清空自有媒体清单（不会删除原始 JSON 或视频文件）。");
      setConfirmManifestClear(false);
      refreshCatalog();
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setManifestBusy(false);
    }
  }

  async function play(seriesId: string, episodeId: string) {
    if (!detail || detail.series.id !== seriesId) {
      setError("选集数据已失效，请重新进入详情");
      return;
    }
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
      // Refresh detail before playing so deleted/replaced episodes never use stale metadata.
      const current = await getCatalogSeries(seriesId);
      if (current.series.id !== seriesId ||
          !current.episodes.some(item => item.id === episodeId)) {
        throw new Error("此剧集已失效，请刷新内容后重试");
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

  async function playRecentLocal(id: number): Promise<boolean> {
    if (recentPlayRef.current || working) return false;
    recentPlayRef.current = true;
    setWorking(true);
    setError("");
    try {
      return await onPlayLocal(id, [id], "最近播放");
    } catch (reason) {
      setError(String(reason));
      return false;
    } finally {
      recentPlayRef.current = false;
      setWorking(false);
    }
  }

  const lastProgress = detail ? latestEpisodeProgress(detail.episodes, episodeProgress) : null;
  const lastEpisode = lastProgress && detail?.episodes.find(item => item.id === lastProgress.episodeId);
  const sortedEpisodes = detail ? [...detail.episodes].sort((a, b) => a.number - b.number) : [];
  const nextEpisode = lastEpisode && lastProgress?.completed
    ? sortedEpisodes[sortedEpisodes.findIndex(item => item.id === lastEpisode.id) + 1] : undefined;
  const playedEpisodes = detail
    ? watchedEpisodeCount(detail.episodes.map(item => item.id), episodeProgress) : 0;
  const communityPlayEnabled = detail?.series.providerId !== "hgcommunity" ||
    sources.some(provider => provider.id === "hgcommunity" && provider.supportsPlay);
  const firstEpisode = sortedEpisodes[0];
  const explorerGenres = Array.from(new Set(items.map(item => item.genre.trim()).filter(Boolean)))
    .sort((a, b) => a.localeCompare(b, "zh-CN"));
  const visibleItems = genreFilter === "all" ? items : items.filter(item => item.genre.trim() === genreFilter);
  const sourceLabel = sources.find(source => source.id === sourceId)?.name ??
    (sourceId === "all" ? "全部内容源" : sourceId === "demo" ? "演示内容" :
      sourceId === "hongguo" ? "红果接入" : sourceId === "hgcommunity" ? "社区接口实验" :
        sourceId === "manifest" ? "自有媒体清单" : sourceId);
  const sidebarSources = sources.length ? sources.filter(source => source.id !== "local") : [
    { id: "demo", name: "演示内容", supportsSearch: true, supportsDetail: true, supportsPlay: true },
    { id: "hongguo", name: "红果接入", supportsSearch: false, supportsDetail: false, supportsPlay: false },
  ];

  return <section className="catalog-view catalog-layout" aria-label="内容剧库">
    <aside className="catalog-sidebar" aria-label="剧库导航">
      <div className="catalog-sidebar-brand">
        <span className="catalog-sidebar-mark" aria-hidden="true">H</span>
        <span><strong>内容空间</strong><small>PERSONAL LIBRARY</small></span>
      </div>

      <nav className="catalog-sidebar-group" aria-label="发现">
        <span className="catalog-sidebar-label">发现</span>
        <button type="button" className="catalog-side-link"
          aria-label="首页" aria-current={mode === "home" ? "page" : undefined}
          onClick={() => { setMode("home"); setError(""); setDetail(null); }}>
          <span className="catalog-side-icon" aria-hidden="true">⌂</span><span>首页</span>
        </button>
        <button type="button" className="catalog-side-link"
          aria-label="探索全部内容" aria-current={mode === "demo" && sourceId === "all" ? "page" : undefined}
          onClick={() => selectSource("all")}>
          <span className="catalog-side-icon" aria-hidden="true">⌕</span><span>探索全部</span>
        </button>
      </nav>

      <nav className="catalog-sidebar-group" aria-label="我的媒体">
        <span className="catalog-sidebar-label">我的媒体</span>
        <button type="button" className="catalog-side-link"
          aria-label="我的视频" aria-current={mode === "local" ? "page" : undefined}
          onClick={() => { setMode("local"); setError(""); }}>
          <span className="catalog-side-icon" aria-hidden="true">▣</span><span>我的视频</span>
        </button>
        <button type="button" className="catalog-side-link"
          aria-label="最近播放" aria-current={mode === "history" ? "page" : undefined}
          onClick={() => { setMode("history"); setError(""); }}>
          <span className="catalog-side-icon" aria-hidden="true">◷</span><span>最近播放</span>
        </button>
        <button type="button" className="catalog-side-link"
          aria-label="我的收藏" aria-current={mode === "favorites" ? "page" : undefined}
          onClick={() => { setMode("favorites"); setError(""); }}>
          <span className="catalog-side-icon" aria-hidden="true">☆</span><span>我的收藏</span>
        </button>
      </nav>

      <nav className="catalog-sidebar-group catalog-sidebar-sources" aria-label="内容来源">
        <span className="catalog-sidebar-label">内容来源</span>
        {sidebarSources.map(provider => <button type="button" key={provider.id}
          className="catalog-side-link catalog-side-source"
          aria-label={provider.name}
          aria-current={mode === "demo" && sourceId === provider.id ? "page" : undefined}
          onClick={() => selectSource(provider.id)}>
          <span className="catalog-side-source-dot" aria-hidden="true" />
          <span>{provider.name}</span>
          {provider.id === "hgcommunity" && <small>实验</small>}
        </button>)}
      </nav>

      <div className="catalog-sidebar-footer">
        <span className="catalog-sidebar-status-dot" aria-hidden="true" />
        <span><strong>本机媒体中心</strong><small>内容与观看记录保存在本机</small></span>
      </div>
    </aside>

    <div className={"catalog-main" + (!detail && mode === "home" ? " catalog-main-home" : "")}>
      <div className={"catalog-heading" + (detail ? " catalog-heading-detail" : "")}>
        {detail ? <div className="catalog-breadcrumb">
          <button type="button" className="catalog-back-link" disabled={working}
            onClick={() => setDetail(null)}>← 返回内容</button>
          <span>作品详情</span>
        </div> : <div>
          <p className="catalog-eyebrow">HONGGUORUST · MEDIA CENTER</p>
          <h2>{mode === "home" ? "首页" : mode === "favorites" ? "我的收藏"
            : mode === "history" ? "最近播放" : mode === "local" ? "我的视频"
              : sourceId === "all" ? "探索全部内容" : sourceLabel}</h2>
          <p>{mode === "home" ? "从你的内容来源中发现作品，继续上次观看，或浏览完整片库。"
            : mode === "history"
              ? "查看本机保存的观看记录与播放进度。"
              : mode === "favorites" ? "集中管理你收藏的本地视频。"
                : mode === "local" ? "管理你主动添加的本地 MP4 文件。"
                  : "搜索、筛选并浏览已接入内容源返回的真实作品。"}</p>
        </div>}
      </div>
    {!detail && mode === "demo" && <div className="catalog-source-tools">
      <div className="catalog-source-current">
        <small>当前浏览来源</small>
        <strong>{sourceLabel}</strong>
      </div>
      <div className="catalog-source-actions">
        <button type="button" className="catalog-ghost"
          disabled={working || loadingMore} onClick={refreshCatalog}>刷新内容</button>
        <button type="button" className="catalog-ghost"
          disabled={!(loading || loadingMore || detailLoading)} onClick={cancelPending}>取消请求</button>
        {(sourceId === "manifest" || sourceId === "all") && <>
          <button type="button" className="catalog-ghost"
            disabled={manifestBusy || working} onClick={() => void importManifest()}>
            {manifestBusy ? "处理中..." : "导入 JSON 清单"}
          </button>
          <button type="button" className="catalog-ghost"
            disabled={manifestBusy || working} onClick={() => void clearManifest()}>
            {confirmManifestClear ? "再次点击确认清空" : "清空自有清单"}
          </button>
        </>}
      </div>
      {!loading && <span className="catalog-source-count" role="status">共 {total} 部作品</span>}
    </div>}

    {!detail && mode === "demo" && sourceId === "hgcommunity" && <div className="catalog-manifest-info" role="status">
      <strong>HG-2A · 非官方社区接口实验（需本机主动启用）</strong>
      <p>真实数据通过本机 openhongguo 社区 SDK 获取，非红果官方服务。
        接口可能失效或限流；受保护媒体不在实验模式中解密。
        若接口失败将保留错误提示，不自动伪造剧集或回退演示资源。</p>
    </div>}
    {!detail && mode === "demo" && sourceId === "hongguo" && <div className="catalog-manifest-info" role="status">
      <strong>HG-1 · 红果接入准备中（尚未授权）</strong>
      <p>已在 Rust 中注册独立的 HongGuo Provider，但目前没有可确认授权的红果剧集接口或播放权限。
        此处不会显示虚构的真实作品，也不会自动抓取平台内容。后续接入需先确认接口访问、剧集分发及 Windows 播放授权。</p>
    </div>}

    {!detail && mode === "demo" && sourceId !== "hongguo" && <form className="catalog-search" onSubmit={submit}>
      <span className="catalog-search-icon" aria-hidden="true">⌕</span>
      <input type="search" value={input} onChange={event => setInput(event.target.value)}
        placeholder="搜索作品名称、类型或关键词" aria-label="搜索内容" maxLength={128} />
      <button type="submit">搜索</button>
    </form>}
    {!detail && mode === "demo" && items.length > 0 && <div className="catalog-explorer-filters">
      <span>类型筛选</span>
      <div className="catalog-explorer-chips">
        <button type="button" aria-pressed={genreFilter === "all"}
          className={genreFilter === "all" ? "active" : ""}
          onClick={() => setGenreFilter("all")}>全部类型</button>
        {explorerGenres.map(name => <button type="button" key={name}
          aria-pressed={genreFilter === name}
          className={genreFilter === name ? "active" : ""}
          onClick={() => setGenreFilter(name)}>{name}</button>)}
      </div>
      <small>显示 {visibleItems.length} / {total} 部</small>
    </div>}

    {manifestInfo && <p className="catalog-manifest-info" role="status">{manifestInfo}</p>}
    {error && <div className="catalog-request-error" role="alert">
      <span>{error}</span>
      <button type="button" className="catalog-ghost"
        onClick={refreshCatalog}>重试加载</button>
    </div>}
    {playerError && <p role="alert" className="catalog-error">{playerError}</p>}

    {mode === "local" || mode === "favorites"
      ? <LocalLibraryView key={mode} favoritesOnly={mode === "favorites"}
          onPlay={onPlayLocal} playerError={playerError} />
      : mode === "history" ? <>
          <ContinueWatchingShelf includeLocal={false} limit={24} busy={working} refreshKey={refreshKey}
            onPlayProvider={playRecentProvider} onPlayLocal={playRecentLocal} />
          <h3 className="discovery-section-title">本地媒体观看历史</h3>
          <LocalLibraryView key={mode} historyOnly onPlay={onPlayLocal} playerError={playerError} />
        </>
      : detail ? <div className="catalog-detail">
      <div className={"catalog-detail-backdrop artwork-" + detail.series.artwork} aria-hidden="true">
        <CoverImage url={detail.series.coverUrl} diagnostic={detail.series.providerId === "hgcommunity"} eager priority />
      </div>
      <div className="catalog-detail-layout">
        <div className={"catalog-detail-poster artwork-" + detail.series.artwork} aria-hidden="true">
          <CoverImage url={detail.series.coverUrl} diagnostic={detail.series.providerId === "hgcommunity"} eager />
          <span>{detail.series.providerId === "manifest" ? "自有清单"
            : detail.series.providerId === "local" ? "本地媒体"
              : detail.series.providerId === "hgcommunity" ? "社区 SDK · 实验" : "演示内容"}</span>
        </div>
        <div className="catalog-detail-body">
          <div className="catalog-detail-kicker">
            <span>{sources.find(source => source.id === detail.series.providerId)?.name ?? "已接入内容"}</span>
            <span>{detail.series.genre}</span>
            <span>{detail.series.badge}</span>
          </div>
          <h1>{detail.series.title}</h1>
          <p className="catalog-detail-description">{detail.series.description}</p>
          <div className="catalog-detail-facts">
            <span>{detail.episodes.length} 集</span>
            {detail.series.providerId !== "local" && <span>已观看 {playedEpisodes} 集</span>}
            <span>{communityPlayEnabled ? "可查看剧集状态" : "仅浏览模式"}</span>
          </div>
          {!lastEpisode && firstEpisode && <button type="button" className="catalog-primary-action"
            disabled={working || !communityPlayEnabled || firstEpisode.playable === false}
            onClick={() => void play(detail.series.id, firstEpisode.id)}>
            {firstEpisode.playable === false ? "首集暂不可播放" : "从第 " + firstEpisode.number + " 集开始"}
          </button>}
        {detail.series.providerId === "hgcommunity" && !communityPlayEnabled &&
          <p className="catalog-availability-info" role="status">
            当前为真实剧集目录浏览模式。尚未配置允许的媒体域名，不会尝试解析或播放视频。
          </p>}
        {progressError && <p className="catalog-progress-error" role="alert">
          观看历史读取失败，原有记录不会被自动覆盖：{progressError}
        </p>}
        {lastEpisode && lastProgress && <div className="catalog-resume" role="status">
          <div>
            <strong>上次观看：第 {lastEpisode.number} 集 · {lastEpisode.title}</strong>
            <span>{lastProgress.completed
              ? "本集已观看完毕"
              : "播放进度 " + formatEpisodePosition(lastProgress.positionSec) +
                (lastProgress.durationSec ? " / " + formatEpisodePosition(lastProgress.durationSec) : "")}
            </span>
          </div>
          <button type="button" disabled={working || !communityPlayEnabled || lastEpisode.playable === false}
            onClick={() => void play(detail.series.id, lastEpisode.id)}>
            {lastProgress.completed ? "重看本集" : "继续观看"}
          </button>
        </div>}
        {nextEpisode && <button type="button" className="catalog-next-episode"
          disabled={working || !communityPlayEnabled || nextEpisode.playable === false}
          onClick={() => void play(detail.series.id, nextEpisode.id)}>
          上次观看的剧集已完播 · 播放下一集（第 {nextEpisode.number} 集）
        </button>}
        <h3 className="catalog-episodes-heading">剧集列表 <span>共 {detail.episodes.length} 集
          {detail.series.providerId !== "local" && (progressLoading
            ? " · 正在读取观看记录" : progressError ? " · 历史不可用"
              : " · 已观看 " + playedEpisodes + " 集")}
          {" · "}{communityPlayEnabled ? "可用剧集点击播放后可切换上一集/下一集" : "当前仅浏览，不开放播放"}</span></h3>
        <div className="catalog-episodes">
          {sortedEpisodes.map(episode => {
            const saved = episodeProgress.find(item => item.episodeId === episode.id);
            const percent = saved ? watchedPercent(saved.positionSec, saved.durationSec, saved.completed) : null;
            return <button key={episode.id} type="button"
              className={"catalog-episode-card" + (episode.playable === false ? " is-unavailable" : "") +
                (saved?.completed ? " is-complete" : "")}
              disabled={working || !communityPlayEnabled || episode.playable === false}
              title={episode.playable === false ? "上游标记该集不可播放" : !communityPlayEnabled ? "当前仅可浏览" : undefined}
              onClick={() => void play(detail.series.id, episode.id)}>
              <strong>{episode.number.toString().padStart(2, "0")}</strong>
              <span>{episode.title}</span>
              <small>{episode.playable === false ? "上游标记不可播放" : !communityPlayEnabled
                ? "仅可浏览" : saved
                ? saved.completed ? "✓ 已看完" : "上次看到 " + formatEpisodePosition(saved.positionSec)
                : episode.runtimeLabel}</small>
              {percent !== null && <span className="catalog-episode-progress" role="progressbar"
                aria-label={"第 " + episode.number + " 集观看进度"} aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={percent}><i style={{ width: percent + "%" }} /></span>}
            </button>;
          })}
        </div>
      </div>
        </div>
      </div> : mode === "home"
      ? <DiscoveryHome sources={sources} busy={working}
          onSelect={id => void showDetail(id)} onExplore={openExplorer}
          onPlayProvider={playRecentProvider} onPlayLocal={playRecentLocal} />
      : loading ? <p className="catalog-empty" role="status">正在加载内容…</p>
      : items.length === 0 ? <p className="catalog-empty">
        {sourceId === "hongguo"
          ? "红果接入接口尚未授权，当前不提供真实作品搜索或播放。已完成可扩展的 Provider 接入入口。"
          : "没有匹配的内容，试试“城市”或“自然”。"}
      </p>
      : <div className="catalog-results">
        <div className="catalog-results-heading">
          <div><h3>全部作品</h3><p>按照当前内容源返回顺序展示，不代表热度或评分排名。</p></div>
          <span>{visibleItems.length} 部</span>
        </div>
        <div className="catalog-grid">
          {visibleItems.map((item, index) => <button key={item.id} type="button"
            className="catalog-card" disabled={working} onClick={() => void showDetail(item.id)}>
            <span className={"catalog-artwork artwork-" + item.artwork} aria-hidden="true">
              <CoverImage url={item.coverUrl} diagnostic={item.providerId === "hgcommunity"}
                eager={index < 10} priority={index < 3} />
              <span className="catalog-card-source">{item.providerId === "manifest" ? "自有清单"
                : item.providerId === "local" ? "本地媒体"
                  : item.providerId === "hgcommunity" ? "社区 SDK · 实验" : "演示内容"}</span>
              <span className="catalog-card-hover">查看详情 <i aria-hidden="true">→</i></span>
            </span>
            <strong>{item.title}</strong>
            <span className="catalog-meta">{item.genre} · {item.episodeCount} 集</span>
            <span className="catalog-card-badge">{item.badge}</span>
          </button>)}
        </div>
      </div>}
    {mode === "demo" && !detail && nextOffset !== null && <div className="catalog-pagination">
      <span>已显示 {items.length} / {total} 条</span>
      <button type="button" disabled={working || loadingMore}
        onClick={() => void loadMore()}>{loadingMore ? "正在加载..." : "加载更多"}</button>
    </div>}
    {mode === "demo" && <p className="catalog-disclaimer">演示内容仅用于功能验证；本地来源只读取你主动添加的 MP4。自有清单仅接收用户选择的 JSON，引用公开 HTTPS MP4，导入前请确认内容使用权限；不会扫描或上传本地视频。</p>}
    </div>
  </section>;
}
