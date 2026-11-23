import { useEffect, useMemo, useRef, useState } from "react";
import { catalogErrorMessage, isCatalogCancellation, searchCatalogPage } from "./api";
import type { CatalogProvider, Series } from "./types";
import { ContinueWatchingShelf } from "./ContinueWatchingShelf";
import { CoverImage } from "./CoverImage";
import {
  discoveryGenres, discoveryHero, discoveryItems, discoverySourceLabel,
  visibleCatalogProviders, visibleCatalogSeries,
} from "./discovery";

interface Props {
  sources: CatalogProvider[];
  busy: boolean;
  refreshSignal: number;
  favoriteIds: ReadonlySet<string>;
  onToggleFavorite: (seriesId: string) => Promise<void>;
  onSelect: (id: string) => void;
  onQuickPlay: (series: Series) => void;
  onExplore: (sourceId: string, query: string) => void;
  onPlayProvider: (seriesId: string, episodeId: string) => Promise<boolean>;
  onPlayLocal: (id: number, title?: string) => Promise<boolean>;
  onOpenLocalFile: () => Promise<void>;
  onAddLocalMedia: () => Promise<void>;
  onImportManifest: () => Promise<void>;
}

const PAGE_SIZE = 50;

function PosterRail({
  title, items, names, busy, favoriteIds, onToggleFavorite, onSelect,
}: {
  title: string;
  items: Series[];
  names: ReadonlyMap<string, string>;
  busy: boolean;
  favoriteIds: ReadonlySet<string>;
  onToggleFavorite: (seriesId: string) => Promise<void>;
  onSelect: (id: string) => void;
}) {
  const trackRef = useRef<HTMLDivElement | null>(null);

  function scroll(direction: -1 | 1) {
    const el = trackRef.current;
    if (!el) return;
    el.scrollBy({ left: direction * Math.max(380, el.clientWidth * 0.82), behavior: "smooth" });
  }

  return (
    <section className="nf-rail-section" aria-label={title}>
      <div className="nf-rail-header">
        <h3>{title}</h3>
        {items.length > 4 && (
          <div className="nf-rail-arrows">
            <button type="button" aria-label={`向左滚动${title}`} onClick={() => scroll(-1)}>‹</button>
            <button type="button" aria-label={`向右滚动${title}`} onClick={() => scroll(1)}>›</button>
          </div>
        )}
      </div>
      <div className="nf-poster-rail" ref={trackRef}>
        {items.map((item, index) => {
          const isFav = favoriteIds.has(item.id);
          return (
            <div key={item.id} className="nf-poster-card-wrap is-rail-item">
              <button
                type="button"
                className="nf-poster-card"
                disabled={busy}
                onClick={() => onSelect(item.id)}
              >
                <span className={"nf-poster-frame artwork-" + item.artwork} aria-hidden="true">
                  <span className="nf-poster-fallback-title">{item.title}</span>
                  <CoverImage url={item.coverUrl} seriesId={item.id} eager={index < 6} priority={index === 0} />
                  <span className="nf-poster-gradient" />
                  <span className="nf-poster-badge">{item.episodeCount > 1 ? `全 ${item.episodeCount} 集` : discoverySourceLabel(item.providerId, names)}</span>
                  <span className="nf-poster-hover-play">
                    <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                  </span>
                </span>
                <span className="nf-poster-info">
                  <strong title={item.title}>{item.title}</strong>
                  <small>{item.genre || discoverySourceLabel(item.providerId, names)}</small>
                </span>
              </button>
              <button
                type="button"
                className={"nf-poster-fav-btn" + (isFav ? " is-favorited" : "")}
                disabled={busy}
                aria-pressed={isFav}
                aria-label={(isFav ? "取消收藏 " : "收藏 ") + item.title}
                title={isFav ? "取消收藏" : "加入我的收藏"}
                onClick={e => {
                  e.stopPropagation();
                  void onToggleFavorite(item.id);
                }}
              >
                {isFav ? "★" : "☆"}
              </button>
            </div>
          );
        })}
      </div>
    </section>
  );
}

export function DiscoveryHome({
  sources, busy, refreshSignal, favoriteIds, onToggleFavorite, onSelect, onQuickPlay, onExplore,
  onPlayProvider, onPlayLocal, onOpenLocalFile, onAddLocalMedia, onImportManifest,
}: Props) {
  const realSources = useMemo(() => visibleCatalogProviders(sources), [sources]);
  const [source, setSource] = useState("all");
  const [genre, setGenre] = useState("all");
  const [items, setItems] = useState<Series[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState("");
  const [localRefresh, setLocalRefresh] = useState(0);
  const requestVersion = useRef(0);
  const requestController = useRef<AbortController | null>(null);
  const pageController = useRef<AbortController | null>(null);

  const combinedRefresh = refreshSignal + localRefresh;

  useEffect(() => {
    const version = ++requestVersion.current;
    const controller = new AbortController();
    requestController.current = controller;
    pageController.current?.abort();
    setLoadingMore(false);
    setLoading(true);
    setError("");
    setItems([]);
    setNextOffset(null);
    void searchCatalogPage("", source, 0, PAGE_SIZE, { signal: controller.signal })
      .then(page => {
        if (version !== requestVersion.current || controller.signal.aborted) return;
        setItems(visibleCatalogSeries(page.items));
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
  }, [source, combinedRefresh]);

  useEffect(() => () => {
    pageController.current?.abort();
  }, []);

  const names = useMemo(() => new Map(
    realSources.map(item => [item.id, item.name]),
  ), [realSources]);

  const genres = useMemo(() => discoveryGenres(items), [items]);
  const currentGenre = genre === "all" || genres.includes(genre) ? genre : "all";
  const visible = useMemo(() => discoveryItems(items, currentGenre), [items, currentGenre]);
  const hero = useMemo(() => discoveryHero(visible), [visible]);
  const heroPlayable = useMemo(() => {
    if (!hero) return false;
    const provider = realSources.find(p => p.id === hero.providerId);
    return provider ? provider.supportsPlay : true;
  }, [hero, realSources]);

  const rails = useMemo(() => {
    const grouped = genres
      .map(name => ({
        title: name,
        items: visible.filter(item => item.genre.trim() === name),
      }))
      .filter(row => row.items.length > 0);
    if (grouped.length > 0) return grouped;
    if (visible.length > 0) return [{ title: "我的片库", items: visible }];
    return [];
  }, [genres, visible]);

  async function loadMore() {
    if (nextOffset === null || loading || loadingMore) return;
    const version = requestVersion.current;
    const controller = new AbortController();
    pageController.current = controller;
    setLoadingMore(true);
    setError("");
    try {
      const page = await searchCatalogPage("", source, nextOffset, PAGE_SIZE, {
        signal: controller.signal,
      });
      if (version !== requestVersion.current || controller.signal.aborted) return;
      setItems(previous => [...previous, ...visibleCatalogSeries(page.items)]);
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

  return (
    <div className="nf-home">
      {/* Cinematic Hero Billboard */}
      {loading ? (
        <section className="nf-hero nf-hero-skeleton" role="status" aria-label="正在加载首页内容">
          <div className="nf-hero-inner">
            <div className="nf-skeleton-line is-sm" />
            <div className="nf-skeleton-line is-lg" />
            <div className="nf-skeleton-line is-md" />
          </div>
        </section>
      ) : hero ? (
        <section className="nf-hero" aria-label={"精选推荐：" + hero.title}>
          <div className={"nf-hero-backdrop artwork-" + hero.artwork} aria-hidden="true">
            <CoverImage url={hero.coverUrl} seriesId={hero.id} eager priority />
          </div>
          <div className="nf-hero-vignette" aria-hidden="true" />
          <div className="nf-hero-inner">
            <div className="nf-hero-copy">
              <div className="nf-hero-kicker">
                <span className="nf-kicker-bar" aria-hidden="true" />
                <span>{hero.genre || discoverySourceLabel(hero.providerId, names)}</span>
              </div>
              <h1>{hero.title}</h1>
              <div className="nf-hero-meta">
                {hero.episodeCount > 1 && <span>全 {hero.episodeCount} 集</span>}
                <span>{discoverySourceLabel(hero.providerId, names)}</span>
                {hero.badge && <span>{hero.badge}</span>}
              </div>
              <p className="nf-hero-synopsis">
                {hero.description || "点击下方按钮立即开始观看，或查看完整选集与观看进度。"}
              </p>
              <div className="nf-hero-actions">
                <button
                  type="button"
                  className="nf-btn-play"
                  disabled={busy}
                  onClick={() => (heroPlayable ? onQuickPlay(hero) : onSelect(hero.id))}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12.5-7.5z" fill="currentColor" /></svg>
                  {heroPlayable ? "立即观看" : "查看剧集"}
                </button>
                <button
                  type="button"
                  className="nf-btn-secondary"
                  disabled={busy}
                  onClick={() => onSelect(hero.id)}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" strokeWidth="2" />
                    <path d="M12 11v5m0-8h.01" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
                  </svg>
                  更多信息
                </button>
                <button
                  type="button"
                  className={"nf-btn-secondary nf-btn-fav" + (favoriteIds.has(hero.id) ? " is-favorited" : "")}
                  disabled={busy}
                  aria-pressed={favoriteIds.has(hero.id)}
                  onClick={() => void onToggleFavorite(hero.id)}
                >
                  <span>{favoriteIds.has(hero.id) ? "★ 已收藏" : "☆ 收藏"}</span>
                </button>
              </div>
            </div>
            <div
              className={"nf-hero-poster-showcase" + (!hero.coverUrl ? " is-landscape" : "")}
              aria-hidden="true"
              onClick={() => (heroPlayable ? onQuickPlay(hero) : onSelect(hero.id))}
            >
              <CoverImage url={hero.coverUrl} seriesId={hero.id} eager priority />
              <span className="nf-hero-showcase-play">
                <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
              </span>
            </div>
          </div>
        </section>
      ) : (
        <section className="nf-hero nf-hero-welcome" aria-label="欢迎使用">
          <div className="nf-hero-vignette" aria-hidden="true" />
          <div className="nf-hero-inner">
            <div className="nf-hero-copy">
              <div className="nf-hero-kicker">
                <span className="nf-kicker-bar" aria-hidden="true" />
                <span>DESKTOP CINEMA</span>
              </div>
              <h1>你的桌面短剧影院</h1>
              <p className="nf-hero-synopsis">
                原生硬件加速播放、无边框沉浸叠加控制条、自动断点续播与清晰剧集管理。立即选择本地影片或导入你的剧集清单开始观看。
              </p>
              <div className="nf-hero-actions">
                <button type="button" className="nf-btn-play" disabled={busy}
                  onClick={() => void onOpenLocalFile()}>
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12.5-7.5z" fill="currentColor" /></svg>
                  打开本地视频
                </button>
                <button type="button" className="nf-btn-secondary" disabled={busy}
                  onClick={() => void onAddLocalMedia()}>
                  ＋ 添加到片库
                </button>
                <button type="button" className="nf-btn-secondary" disabled={busy}
                  onClick={() => void onImportManifest()}>
                  接入 WebDAV / 订阅源
                </button>
              </div>
            </div>
          </div>
        </section>
      )}

      {/* Main Rails Container */}
      <div className="nf-home-rows">
        {error && (
          <div className="nf-alert-banner" role="alert">
            <span>{error}</span>
            <button type="button" onClick={() => setLocalRefresh(v => v + 1)}>重试</button>
          </div>
        )}

        {/* Filter Bar (only when multiple sources or genres exist) */}
        {!loading && (realSources.length > 1 || genres.length > 1) && (
          <div className="nf-filter-strip">
            {realSources.length > 1 && (
              <div className="nf-chip-group" role="group" aria-label="按内容来源筛选">
                <button type="button" className={source === "all" ? "is-active" : ""}
                  onClick={() => { setSource("all"); setGenre("all"); }}>全部</button>
                {realSources.map(p => (
                  <button key={p.id} type="button" className={source === p.id ? "is-active" : ""}
                    onClick={() => { setSource(p.id); setGenre("all"); }}>{p.name}</button>
                ))}
              </div>
            )}
            {genres.length > 1 && (
              <div className="nf-chip-group" role="group" aria-label="按题材筛选">
                <button type="button" className={currentGenre === "all" ? "is-active" : ""}
                  onClick={() => setGenre("all")}>全部题材</button>
                {genres.map(name => (
                  <button key={name} type="button" className={currentGenre === name ? "is-active" : ""}
                    onClick={() => setGenre(name)}>{name}</button>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Continue Watching Rail */}
        {!loading && (
          <ContinueWatchingShelf
            includeLocal
            limit={10}
            busy={busy}
            refreshKey={combinedRefresh}
            onPlayProvider={onPlayProvider}
            onPlayLocal={onPlayLocal}
          />
        )}

        {/* Genre Poster Rails */}
        {!loading && rails.map(row => (
          <PosterRail
            key={row.title}
            title={row.title}
            items={row.items}
            names={names}
            busy={busy}
            favoriteIds={favoriteIds}
            onToggleFavorite={onToggleFavorite}
            onSelect={onSelect}
          />
        ))}

        {/* Load more / Explore footer */}
        {!loading && (nextOffset !== null || items.length > 0) && (
          <div className="nf-home-footer-actions">
            {nextOffset !== null && (
              <button type="button" className="nf-btn-secondary" disabled={loadingMore || busy}
                onClick={() => void loadMore()}>
                {loadingMore ? "正在加载…" : "加载更多剧集"}
              </button>
            )}
            {items.length > 0 && (
              <button type="button" className="nf-btn-ghost"
                onClick={() => onExplore(source, "")}>
                浏览全部片库 →
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
