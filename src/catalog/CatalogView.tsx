import { useEffect, useRef, useState, type FormEvent } from "react";
import { getCatalogSeries, listCatalogSources, searchCatalogPage } from "./api";
import type { CatalogProvider, Series, SeriesDetail } from "./types";
import { LocalLibraryView } from "./LocalLibraryView";

interface Props {
  onBack: () => Promise<void>;
  onPlay: (seriesId: string, episodeId: string) => Promise<boolean>;
  onPlayLocal: (id: number, orderedIds: number[], origin: string) => Promise<boolean>;
  playerError: string;
}

export function CatalogView({ onBack, onPlay, onPlayLocal, playerError }: Props) {
  const [mode, setMode] = useState<"demo" | "local" | "history" | "favorites">("demo");
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
  const [detail, setDetail] = useState<SeriesDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);

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
    const version = ++requestVersion.current;
    setLoading(true);
    setError("");
    setDetail(null);
    setItems([]);
    setTotal(0);
    setNextOffset(null);
    void searchCatalogPage(search, sourceId).then(page => {
      if (version !== requestVersion.current) return;
      setItems(page.items);
      setTotal(page.total);
      setNextOffset(page.nextOffset);
    }).catch(reason => {
      if (version === requestVersion.current) setError(String(reason));
    }).finally(() => {
      if (version === requestVersion.current) setLoading(false);
    });
    return () => {
      if (version === requestVersion.current) requestVersion.current++;
    };
  }, [search, sourceId, refreshKey]);

  async function loadMore() {
    if (nextOffset === null || loadingMore || working) return;
    const version = requestVersion.current;
    setLoadingMore(true);
    setError("");
    try {
      const page = await searchCatalogPage(search, sourceId, nextOffset);
      if (version !== requestVersion.current) return;
      setItems(previous => [...previous, ...page.items]);
      setTotal(page.total);
      setNextOffset(page.nextOffset);
    } catch (reason) {
      if (version === requestVersion.current) setError(String(reason));
    } finally {
      setLoadingMore(false);
    }
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setDetail(null);
    setSearch(input.trim());
  }

  async function showDetail(id: string) {
    setError("");
    setWorking(true);
    try {
      setDetail(await getCatalogSeries(id));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  async function play(seriesId: string, episodeId: string) {
    setWorking(true);
    try {
      await onPlay(seriesId, episodeId);
    } finally {
      setWorking(false);
    }
  }

  return <section className="catalog-view" aria-label="内容剧库">
    <div className="catalog-heading">
      <div>
        <p className="catalog-eyebrow">M9 · CONTENT PROVIDERS</p>
        <h2>{detail ? detail.series.title : mode === "favorites" ? "我的收藏" : mode === "history" ? "最近播放" : mode === "local" ? "我的视频" : "探索内容"}</h2>
        <p>{mode === "history"
          ? "播放进度由 Rust 播放器记录，重新播放会从上次位置继续。"
          : mode === "favorites" ? "收藏你自己的本地视频，独立保存观看进度。"
          : mode === "local" ? "管理你自己选择的本地 MP4，文件始终保留在原始位置。"
          : "通过统一 Content Provider 检索演示与本机媒体；演示条目仅使用公开测试素材。"}</p>
      </div>
      <button className="catalog-ghost" disabled={working} onClick={() => { if (detail) setDetail(null); else void onBack(); }}>
        {detail ? "← 返回列表" : "← 返回播放器"}
      </button>
    </div>

    {!detail && <nav className="catalog-tabs" aria-label="内容来源">
      <button type="button" className={mode === "demo" ? "catalog-tab active" : "catalog-tab"}
        onClick={() => { setMode("demo"); setError(""); }}>演示内容</button>
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
        onChange={event => { setSourceId(event.target.value); setInput(""); setSearch(""); }}>
        {sources.length === 0 && <option value="demo">演示内容</option>}
        {sources.map(source => <option key={source.id} value={source.id}>{source.name}</option>)}
        <option value="all">全部内容源</option>
      </select>
      <button type="button" className="catalog-ghost"
        disabled={working || loadingMore}
        onClick={() => setRefreshKey(previous => previous + 1)}>刷新内容</button>
      {!loading && <span role="status">共 {total} 条内容</span>}
    </div>}

    {!detail && mode === "demo" && <form className="catalog-search" onSubmit={submit}>
      <input type="search" value={input} onChange={event => setInput(event.target.value)}
        placeholder="搜索演示剧名、类型或关键词" aria-label="搜索内容" maxLength={128} />
      <button type="submit">搜索</button>
    </form>}

    {(error || playerError) && <p role="alert" className="catalog-error">{error || playerError}</p>}

    {mode === "local" || mode === "history" || mode === "favorites"
      ? <LocalLibraryView key={mode} historyOnly={mode === "history"}
          favoritesOnly={mode === "favorites"} onPlay={onPlayLocal} playerError={playerError} />
      : detail ? <div className="catalog-detail">
      <div className={"catalog-detail-art artwork-" + detail.series.artwork} aria-hidden="true">
        <span>DEMO</span>
      </div>
      <div className="catalog-detail-body">
        <div className="catalog-meta">{detail.series.genre} · {detail.series.badge}</div>
        <p>{detail.series.description}</p>
        <h3>选集 <span>共 {detail.episodes.length} 集</span></h3>
        <div className="catalog-episodes">
          {detail.episodes.map(episode =>
            <button key={episode.id} type="button" disabled={working}
              onClick={() => void play(detail.series.id, episode.id)}>
              <strong>{episode.number.toString().padStart(2, "0")}</strong>
              <span>{episode.title}</span>
              <small>{episode.runtimeLabel}</small>
            </button>
          )}
        </div>
      </div>
    </div> : loading ? <p className="catalog-empty" role="status">正在加载本地演示内容…</p>
      : items.length === 0 ? <p className="catalog-empty">没有匹配的内容，试试“城市”或“自然”。</p>
      : <div className="catalog-grid">
        {items.map(item => <button key={item.id} type="button"
          className="catalog-card" disabled={working} onClick={() => void showDetail(item.id)}>
          <span className={"catalog-artwork artwork-" + item.artwork} aria-hidden="true">
            <span>HGR / DEMO</span>
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
    {mode === "demo" && <p className="catalog-disclaimer">演示内容仅用于功能验证；本地来源只读取你主动添加的 MP4，不上传或扫描文件夹。此功能不接入第三方短剧平台，也不代表真实作品授权。</p>}
  </section>;
}
