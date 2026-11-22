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
  const genreRows = useMemo(() => genres
    .map(name => ({
      name,
      items: visible.filter(item => item.id !== hero?.id && item.genre.trim() === name),
    }))
    .filter(row => row.items.length > 0), [genres, visible, hero]);

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
    <form className="discovery-search" onSubmit={onSearch}>
      <span className="discovery-search-icon" aria-hidden="true">⌕</span>
      <input type="search" value={input} maxLength={128}
        aria-label="首页搜索作品" placeholder="搜索片名、类型或关键词"
        onChange={event => setInput(event.target.value)} />
      <button type="submit" disabled={loading}>搜索</button>
      <button type="button" className="catalog-ghost" disabled={loading}
        onClick={() => setRefreshKey(value => value + 1)}>刷新</button>
    </form>

    <div className="discovery-filter-panel">
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
      {genres.length > 0 && <div className="discovery-filters">
        <span>作品类型</span>
        <div className="discovery-pills" aria-label="首页按真实作品类型筛选">
          <button type="button" aria-pressed={currentGenre === "all"}
            className={currentGenre === "all" ? "active" : ""}
            onClick={() => setGenre("all")}>全部类型</button>
          {genres.map(name => <button type="button" key={name}
            aria-pressed={currentGenre === name}
            className={currentGenre === name ? "active" : ""}
            onClick={() => setGenre(name)}>{name}</button>)}
        </div>
      </div>}
    </div>

    {error && <div className="catalog-request-error" role="alert">
      <span>{error}</span>
      <button type="button" className="catalog-ghost"
        onClick={() => setRefreshKey(value => value + 1)}>重试加载</button>
    </div>}

    {loading ? <p className="catalog-empty" role="status">正在整理已接入的内容…</p>
      : hero ? <>
        <div className="discovery-summary" role="status">
          当前展示 {visible.length} 部 · 已加载 {items.length} / {total} 部作品
        </div>
        <button type="button" disabled={busy} className="discovery-featured"
          onClick={() => onSelect(hero.id)} aria-label={"查看作品详情：" + hero.title}>
          <span className={"discovery-featured-backdrop artwork-" + hero.artwork} aria-hidden="true">
            <CoverImage url={hero.coverUrl} diagnostic={hero.providerId === "hgcommunity"} eager priority />
          </span>
          <span className="discovery-featured-overlay" aria-hidden="true" />
          <span className="discovery-featured-copy">
            <small>内容焦点 · {discoverySourceLabel(hero.providerId, names)}</small>
            <strong>{hero.title}</strong>
            <span className="discovery-featured-meta">{hero.genre} · {hero.episodeCount} 集</span>
            <span className="discovery-featured-description">
              {hero.description || "查看作品详情与已接入的剧集信息。"}
            </span>
            <span className="discovery-featured-cta">查看作品详情 <i aria-hidden="true">→</i></span>
          </span>
          <span className="discovery-featured-footnote">基于当前内容源返回顺序展示，不代表人气排名</span>
        </button>

        <ContinueWatchingShelf includeLocal limit={6} busy={busy} refreshKey={refreshKey}
          onPlayProvider={onPlayProvider} onPlayLocal={onPlayLocal} />

        {genreRows.length > 0 && <div className="discovery-rails">
          {genreRows.map(row => <section className="discovery-rail" key={row.name}>
            <div className="discovery-rail-heading">
              <div><h3>{row.name}</h3><span>按已接入作品类型整理</span></div>
              <small>{row.items.length} 部</small>
            </div>
            <div className="discovery-rail-track">
              {row.items.map((item, index) => <button type="button" key={item.id}
                className="discovery-poster-card" disabled={busy}
                onClick={() => onSelect(item.id)}>
                <span className={"discovery-poster-art artwork-" + item.artwork} aria-hidden="true">
                  <CoverImage url={item.coverUrl} diagnostic={item.providerId === "hgcommunity"}
                    eager={index < 5} priority={index === 0} />
                  <span className="discovery-poster-source">
                    {discoverySourceLabel(item.providerId, names)}
                  </span>
                  <span className="discovery-poster-hover">查看详情 <i aria-hidden="true">→</i></span>
                </span>
                <strong>{item.title}</strong>
                <span className="discovery-poster-meta">{item.episodeCount} 集 · {item.badge}</span>
              </button>)}
            </div>
          </section>)}
        </div>}

        <div className="discovery-bottom-actions">
          <span>内容来自已连接的 Provider；没有生成虚构的榜单或评分。</span>
          <button type="button" className="catalog-ghost"
            onClick={() => onExplore(source, query)}>打开完整探索页</button>
        </div>
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
      <span>已加载 {items.length} / {total} 部</span>
      <button type="button" disabled={loadingMore || busy}
        onClick={() => void loadMore()}>{loadingMore ? "正在加载…" : "加载更多内容"}</button>
    </div>}

    <p className="catalog-disclaimer">首页只使用已连接 Provider 返回的真实元数据；内容焦点与类型分组不代表热度或评分排名。
      分类仅在已加载条目中筛选，可点击“加载更多内容”扩展范围。导入内容及媒体使用权限由用户自行确认。</p>
  </section>;
}
