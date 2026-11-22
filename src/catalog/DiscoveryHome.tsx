import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  catalogErrorMessage, isCatalogCancellation, searchCatalogPage,
} from "./api";
import type { CatalogProvider, Series } from "./types";
import { ContinueWatchingShelf } from "./ContinueWatchingShelf";
import { CoverImage } from "./CoverImage";
import {
  discoveryGenres, discoveryHero, discoveryItems, discoverySourceLabel,
} from "./discovery";

interface Props {
  sources: CatalogProvider[];
  busy: boolean;
  onSelect: (id: string) => void;
  onExplore: (sourceId: string, query: string) => void;
  onPlayProvider: (seriesId: string, episodeId: string) => Promise<boolean>;
  onPlayLocal: (id: number) => Promise<boolean>;
}

const PAGE_SIZE = 50;
const sourceFallback: CatalogProvider[] = [
  { id: "demo", name: "演示内容", supportsSearch: true, supportsDetail: true, supportsPlay: true },
  { id: "local", name: "本地媒体", supportsSearch: true, supportsDetail: true, supportsPlay: true },
  { id: "manifest", name: "自有媒体清单", supportsSearch: true, supportsDetail: true, supportsPlay: true },
];

export function DiscoveryHome({ sources, busy, onSelect, onExplore, onPlayProvider, onPlayLocal }: Props) {
  const [source, setSource] = useState("all");
  const [genre, setGenre] = useState("all");
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<Series[]>([]);
  const [total, setTotal] = useState(0);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState("");
  const [refreshKey, setRefreshKey] = useState(0);
  const requestVersion = useRef(0);
  const requestController = useRef<AbortController | null>(null);
  const pageController = useRef<AbortController | null>(null);

  useEffect(() => {
    const version = ++requestVersion.current;
    const controller = new AbortController();
    requestController.current = controller;
    pageController.current?.abort();
    setLoadingMore(false);
    setLoading(true);
    setError("");
    setItems([]);
    setTotal(0);
    setNextOffset(null);
    void searchCatalogPage(query, source, 0, PAGE_SIZE, { signal: controller.signal })
      .then(page => {
        if (version !== requestVersion.current || controller.signal.aborted) return;
        setItems(page.items);
        setTotal(page.total);
        setNextOffset(page.nextOffset);
      })
      .catch(reason => {
        if (version === requestVersion.current && !isCatalogCancellation(reason)) {
          setError(catalogErrorMessage(reason));
        }
      })
      .finally(() => {
        if (version === requestVersion.current) setLoading(false);
        if (requestController.current === controller) requestController.current = null;
      });
    return () => {
      controller.abort();
      if (version === requestVersion.current) requestVersion.current++;
    };
  }, [source, query, refreshKey]);

  useEffect(() => () => {
    pageController.current?.abort();
  }, []);

  const names = useMemo(() => new Map(
    (sources.length ? sources : sourceFallback).map(item => [item.id, item.name]),
  ), [sources]);
  const genres = useMemo(() => discoveryGenres(items), [items]);
  const currentGenre = genre === "all" || genres.includes(genre) ? genre : "all";
  const visible = useMemo(() => discoveryItems(items, currentGenre), [items, currentGenre]);
  const hero = useMemo(() => discoveryHero(visible), [visible]);
  const others = hero ? visible.filter(item => item.id !== hero.id) : [];

  function onSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setGenre("all");
    setQuery(input.trim());
  }

  function chooseSource(id: string) {
    setSource(id);
    setGenre("all");
  }

  async function loadMore() {
    if (nextOffset === null || loading || loadingMore) return;
    const version = requestVersion.current;
    const controller = new AbortController();
    pageController.current = controller;
    setLoadingMore(true);
    setError("");
    try {
      const page = await searchCatalogPage(query, source, nextOffset, PAGE_SIZE, {
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

  return <section className="discovery-home" aria-label="首页内容发现">
    <div className="discovery-intro">
      <div>
        <p className="catalog-eyebrow">M10 · CONTENT DISCOVERY</p>
        <h3>从已接入的内容中，发现下一部想看的视频</h3>
        <p>汇总演示内容、你导入的本地视频和自有 JSON 清单。不使用虚构榜单或未经授权的片源。</p>
      </div>
      <button type="button" className="discovery-explore"
        onClick={() => onExplore(source, query)}>
        浏览完整目录
      </button>
    </div>

    <ContinueWatchingShelf includeLocal limit={6} busy={busy} refreshKey={refreshKey}
      onPlayProvider={onPlayProvider} onPlayLocal={onPlayLocal} />

    <form className="discovery-search" onSubmit={onSearch}>
      <input type="search" value={input} maxLength={128}
        aria-label="首页搜索作品" placeholder="按作品名称、类型或关键词搜索"
        onChange={event => setInput(event.target.value)} />
      <button type="submit" disabled={loading}>搜索</button>
      <button type="button" className="catalog-ghost" disabled={loading}
        onClick={() => setRefreshKey(value => value + 1)}>刷新首页</button>
    </form>

    <div className="discovery-filters">
      <span>内容来源</span>
      <div className="discovery-pills" aria-label="首页按内容来源筛选">
        <button type="button" aria-pressed={source === "all"}
          className={source === "all" ? "active" : ""}
          onClick={() => chooseSource("all")}>全部</button>
        {(sources.length ? sources : sourceFallback).map(provider =>
          <button type="button" key={provider.id}
            aria-pressed={source === provider.id}
            className={source === provider.id ? "active" : ""}
            onClick={() => chooseSource(provider.id)}>{provider.name}</button>)}
      </div>
    </div>

    <div className="discovery-filters">
      <span>已加载内容分类</span>
      <div className="discovery-pills" aria-label="首页分类筛选">
        <button type="button" aria-pressed={currentGenre === "all"}
          className={currentGenre === "all" ? "active" : ""}
          onClick={() => setGenre("all")}>全部分类</button>
        {genres.map(name => <button type="button" key={name}
          aria-pressed={currentGenre === name}
          className={currentGenre === name ? "active" : ""}
          onClick={() => setGenre(name)}>{name}</button>)}
      </div>
    </div>

    {error && <div className="catalog-request-error" role="alert">
      <span>{error}</span>
      <button type="button" className="catalog-ghost"
        onClick={() => setRefreshKey(value => value + 1)}>重试加载</button>
    </div>}

    {loading ? <p className="catalog-empty" role="status">正在整理已接入的内容…</p>
      : hero ? <>
        <div className="discovery-summary" role="status">
          已加载 {items.length} / {total} 条 · 当前分类 {visible.length} 条
        </div>
        <button type="button" disabled={busy} className="discovery-hero"
          onClick={() => onSelect(hero.id)}>
          <span className={"discovery-hero-art artwork-" + hero.artwork} aria-hidden="true">
            <CoverImage url={hero.coverUrl} />
            <span>{discoverySourceLabel(hero.providerId, names)}</span>
          </span>
          <span className="discovery-hero-copy">
            <small>首页焦点 · 来自现有内容源</small>
            <strong>{hero.title}</strong>
            <span>{hero.genre} · {hero.episodeCount} 集 · {discoverySourceLabel(hero.providerId, names)}</span>
            <span className="discovery-description">{hero.description || "查看作品详情与可用剧集。"}</span>
            <span className="discovery-hero-cta">查看详情与选集</span>
          </span>
        </button>
        {others.length > 0 && <>
          <h3 className="discovery-section-title">更多可浏览内容</h3>
          <div className="catalog-grid">
            {others.map(item => <button type="button" key={item.id}
              className="catalog-card" disabled={busy} onClick={() => onSelect(item.id)}>
              <span className={"catalog-artwork artwork-" + item.artwork} aria-hidden="true">
                <CoverImage url={item.coverUrl} />
                <span>{discoverySourceLabel(item.providerId, names)}</span>
              </span>
              <strong>{item.title}</strong>
              <span className="catalog-meta">{item.genre} · {item.episodeCount} 集</span>
              <span className="catalog-card-badge">{item.badge}</span>
            </button>)}
          </div>
        </>}
      </> : !error && <div className="discovery-empty">
        <p>当前条件下没有可展示的内容。</p>
        <button type="button" className="catalog-ghost"
          onClick={() => {
            setGenre("all");
            setSource("all");
            setInput("");
            setQuery("");
            setRefreshKey(value => value + 1);
          }}>查看全部来源</button>
      </div>}

    {!loading && nextOffset !== null && <div className="catalog-pagination">
      <span>已加载 {items.length} / {total} 条</span>
      <button type="button" disabled={loadingMore || busy}
        onClick={() => void loadMore()}>{loadingMore ? "正在加载…" : "加载更多内容"}</button>
    </div>}

    <p className="catalog-disclaimer">首页只使用已连接 Provider 返回的真实元数据；展示顺序不是人气榜单。
      分类仅在已加载条目中筛选，可点击“加载更多内容”扩展范围。导入内容及媒体使用权限由用户自行确认。</p>
  </section>;
}
