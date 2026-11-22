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
    setError("");
    setDetail(null);
    setMode("demo");
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

  return <section className="catalog-view" aria-label="内容剧库">
    <div className="catalog-heading">
      <div>
        <p className="catalog-eyebrow">M10 · MEDIA EXPERIENCE</p>
        <h2>{detail ? detail.series.title : mode === "home" ? "内容首页"
          : mode === "favorites" ? "我的收藏" : mode === "history" ? "最近播放"
          : mode === "local" ? "我的视频" : "探索内容"}</h2>
        <p>{mode === "home" ? "从已接入的内容中发现作品，按真实来源与分类浏览，直接进入详情选集。"
          : mode === "history"
          ? "显示自有清单、演示剧集和本地 MP4 的最近播放记录；进度保存在本机。"
          : mode === "favorites" ? "收藏你自己的本地视频，独立保存观看进度。"
          : mode === "local" ? "管理你自己选择的本地 MP4，文件始终保留在原始位置。"
          : "通过统一 Content Provider 检索演示与本机媒体；演示条目仅使用公开测试素材。"}</p>
      </div>
      <button className="catalog-ghost" disabled={working} onClick={() => { if (detail) setDetail(null); else void onBack(); }}>
        {detail ? "← 返回列表" : "← 返回播放器"}
      </button>
    </div>

    {!detail && <nav className="catalog-tabs" aria-label="内容来源">
      <button type="button" className={mode === "home" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => { setMode("home"); setError(""); }}>首页</button>
      <button type="button" className={mode === "demo" && sourceId !== "hongguo" && sourceId !== "hgcommunity" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => { setMode("demo"); setSourceId("demo"); setError(""); }}>演示内容</button>
      <button type="button"
        className={mode === "demo" && sourceId === "hongguo" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => {
          setMode("demo"); setSourceId("hongguo"); setInput("");
          setSearch(""); setError("");
        }}>红果接入</button>
      {sources.some(source => source.id === "hgcommunity") && <button type="button"
        className={mode === "demo" && sourceId === "hgcommunity" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => {
          setMode("demo"); setSourceId("hgcommunity"); setInput("");
          setSearch(""); setError("");
        }}>社区接口实验</button>}
      <button type="button" className={mode === "local" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => { setMode("local"); setError(""); }}>我的视频</button>
      <button type="button" className={mode === "history" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => { setMode("history"); setError(""); }}>最近播放</button>
      <button type="button" className={mode === "favorites" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => { setMode("favorites"); setError(""); }}>我的收藏</button>
    </nav>}

    {!detail && mode === "demo" && <div className="catalog-source-tools">
      <label htmlFor="catalog-provider-select">内容源</label>
      <select id="catalog-provider-select" aria-label="选择内容源"
        value={sourceId} disabled={working || loadingMore}
        onChange={event => {
          setSourceId(event.target.value);
          setInput("");
          setSearch("");
          setConfirmManifestClear(false);
          setManifestInfo("");
        }}>
        {sources.length === 0 && <option value="demo">演示内容</option>}
        {sources.map(source => <option key={source.id} value={source.id}>{source.name}</option>)}
        <option value="all">全部内容源</option>
      </select>
      <button type="button" className="catalog-ghost"
        disabled={working || loadingMore}
        onClick={refreshCatalog}>刷新内容</button>
      <button type="button" className="catalog-ghost"
        disabled={!(loading || loadingMore || detailLoading)}
        onClick={cancelPending}>取消请求</button>
      {(sourceId === "manifest" || sourceId === "all") && <>
        <button type="button" className="catalog-ghost"
          disabled={manifestBusy || working}
          onClick={() => void importManifest()}>
          {manifestBusy ? "处理中..." : "导入自有 JSON 清单"}
        </button>
        <button type="button" className="catalog-ghost"
          disabled={manifestBusy || working}
          onClick={() => void clearManifest()}>
          {confirmManifestClear ? "再次点击确认清空" : "清空自有清单"}
        </button>
      </>}
      {!loading && <span role="status">共 {total} 条内容</span>}
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
      <input type="search" value={input} onChange={event => setInput(event.target.value)}
        placeholder="搜索作品名称、类型或关键词" aria-label="搜索内容" maxLength={128} />
      <button type="submit">搜索</button>
    </form>}

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
      <div className={"catalog-detail-art artwork-" + detail.series.artwork} aria-hidden="true">
        <span>{detail.series.providerId === "manifest" ? "自有清单"
          : detail.series.providerId === "local" ? "本地媒体"
           : detail.series.providerId === "hgcommunity" ? "社区 SDK · 实验" : "DEMO"}</span>
      </div>
      <div className="catalog-detail-body">
        <div className="catalog-meta">{detail.series.genre} · {detail.series.badge}</div>
        <p>{detail.series.description}</p>
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
          <button type="button" disabled={working}
            onClick={() => void play(detail.series.id, lastEpisode.id)}>
            {lastProgress.completed ? "重看本集" : "继续观看"}
          </button>
        </div>}
        {nextEpisode && <button type="button" className="catalog-next-episode"
          disabled={working} onClick={() => void play(detail.series.id, nextEpisode.id)}>
          上次观看的剧集已完播 · 播放下一集（第 {nextEpisode.number} 集）
        </button>}
        <h3>选集 <span>共 {detail.episodes.length} 集
          {detail.series.providerId !== "local" && (progressLoading
            ? " · 正在读取观看记录" : progressError ? " · 历史不可用"
              : " · 已观看 " + playedEpisodes + " 集")}
          {" · "}点击播放后可在底部切换上一集/下一集</span></h3>
        <div className="catalog-episodes">
          {sortedEpisodes.map(episode => {
            const saved = episodeProgress.find(item => item.episodeId === episode.id);
            const percent = saved ? watchedPercent(saved.positionSec, saved.durationSec, saved.completed) : null;
            return <button key={episode.id} type="button" disabled={working}
              onClick={() => void play(detail.series.id, episode.id)}>
              <strong>{episode.number.toString().padStart(2, "0")}</strong>
              <span>{episode.title}</span>
              <small>{saved
                ? saved.completed ? "✓ 已看完" : "上次看到 " + formatEpisodePosition(saved.positionSec)
                : episode.runtimeLabel}</small>
              {percent !== null && <span className="catalog-episode-progress" role="progressbar"
                aria-label={"第 " + episode.number + " 集观看进度"} aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={percent}><i style={{ width: percent + "%" }} /></span>}
            </button>;
          })}
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
      : <div className="catalog-grid">
        {items.map(item => <button key={item.id} type="button"
          className="catalog-card" disabled={working} onClick={() => void showDetail(item.id)}>
          <span className={"catalog-artwork artwork-" + item.artwork} aria-hidden="true">
            <span>{item.providerId === "manifest" ? "HGR / 自有清单"
              : item.providerId === "local" ? "HGR / LOCAL"
               : item.providerId === "hgcommunity" ? "社区 SDK / 实验" : "HGR / DEMO"}</span>
          </span>
          <strong>{item.title}</strong>
          <span className="catalog-meta">{item.genre} · {item.episodeCount} 集</span>
          <span className="catalog-card-badge">{item.providerId === "local" ? "本地媒体 · " + item.badge : item.badge}</span>
        </button>)}
      </div>}
    {mode === "demo" && !detail && nextOffset !== null && <div className="catalog-pagination">
      <span>已显示 {items.length} / {total} 条</span>
      <button type="button" disabled={working || loadingMore}
        onClick={() => void loadMore()}>{loadingMore ? "正在加载..." : "加载更多"}</button>
    </div>}
    {mode === "demo" && <p className="catalog-disclaimer">演示内容仅用于功能验证；本地来源只读取你主动添加的 MP4。自有清单仅接收用户选择的 JSON，引用公开 HTTPS MP4，导入前请确认内容使用权限；不会扫描或上传本地视频。</p>}
  </section>;
}
