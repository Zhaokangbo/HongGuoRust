import { useMemo, useState } from "react";
import type { FavoriteSeries, Series } from "./types";
import { CoverImage } from "./CoverImage";
import { LocalLibraryView } from "./LocalLibraryView";
import { isVisibleProvider } from "./discovery";

interface Props {
  favorites: FavoriteSeries[];
  busy: boolean;
  refreshSignal: number;
  onSelectSeries: (id: string) => void;
  onQuickPlaySeries: (series: Series) => void;
  onRemoveFavorite: (seriesId: string) => Promise<void>;
  onPlayLocal: (id: number, orderedIds: number[], origin: string, titles?: Record<string, string>) => Promise<boolean>;
  onGoHome: () => void;
}

type FavTab = "all" | "series" | "local";
type FavSort = "recent" | "title";

function formatAddedAt(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit",
  }).format(new Date(ms));
}

export function FavoritesView({
  favorites, busy, refreshSignal, onSelectSeries, onQuickPlaySeries,
  onRemoveFavorite, onPlayLocal, onGoHome,
}: Props) {
  const [tab, setTab] = useState<FavTab>("all");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<FavSort>("recent");

  const visibleSeries = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = favorites
      .filter(item => isVisibleProvider(item.providerId))
      .filter(item =>
        !q
        || item.title.toLowerCase().includes(q)
        || item.genre.toLowerCase().includes(q)
        || item.description.toLowerCase().includes(q),
      );
    if (sort === "title") {
      return [...filtered].sort((a, b) => a.title.localeCompare(b.title, "zh-CN", { numeric: true }));
    }
    return [...filtered].sort((a, b) => b.addedAtMs - a.addedAtMs);
  }, [favorites, query, sort]);

  return (
    <div className="nf-page-container nf-favorites-page">
      <div className="nf-page-header nf-favorites-header">
        <div>
          <h1>我的收藏</h1>
          <p className="nf-page-subtitle">集中管理你收藏的剧集与本地视频，随时接着看</p>
        </div>
        <div className="nf-chip-group" role="tablist" aria-label="收藏分类">
          <button
            type="button"
            role="tab"
            aria-selected={tab === "all"}
            className={tab === "all" ? "is-active" : ""}
            onClick={() => setTab("all")}
          >
            全部收藏
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "series"}
            className={tab === "series" ? "is-active" : ""}
            onClick={() => setTab("series")}
          >
            剧集收藏 ({favorites.filter(i => isVisibleProvider(i.providerId)).length})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "local"}
            className={tab === "local" ? "is-active" : ""}
            onClick={() => setTab("local")}
          >
            本地视频
          </button>
        </div>
      </div>

      {tab !== "local" && (
        <section className="nf-fav-section" aria-label="收藏的剧集">
          <div className="nf-fav-toolbar">
            <div className="nf-fav-section-title">
              <h2>剧集收藏</h2>
              <span>{visibleSeries.length} 部</span>
            </div>
            <div className="nf-library-filters">
              <input
                type="search"
                className="nf-input-dark"
                placeholder="搜索收藏的剧名或题材…"
                aria-label="搜索收藏剧集"
                value={query}
                onChange={e => setQuery(e.target.value)}
              />
              <select
                className="nf-select-dark"
                aria-label="收藏排序"
                value={sort}
                onChange={e => setSort(e.target.value as FavSort)}
              >
                <option value="recent">最近收藏</option>
                <option value="title">按标题排序</option>
              </select>
              {query && (
                <button type="button" className="nf-btn-ghost" onClick={() => setQuery("")}>
                  清除搜索
                </button>
              )}
            </div>
          </div>

          {visibleSeries.length === 0 ? (
            <div className="nf-empty-panel">
              <h3>{query ? "没有找到匹配的收藏剧集" : "还没有收藏的剧集"}</h3>
              <p>
                {query
                  ? "换个关键词试试，或清除搜索查看全部收藏。"
                  : "在首页、片库或剧集详情页点击「☆ 收藏」，喜欢的剧就会保存在这里。"}
              </p>
              {query ? (
                <button type="button" className="nf-btn-secondary" onClick={() => setQuery("")}>
                  查看全部收藏
                </button>
              ) : (
                <button type="button" className="nf-btn-play" onClick={onGoHome}>
                  去首页发现作品
                </button>
              )}
            </div>
          ) : (
            <div className="nf-poster-grid">
              {visibleSeries.map((item, idx) => (
                <div key={item.id} className="nf-poster-card-wrap">
                  <button
                    type="button"
                    className="nf-poster-card"
                    disabled={busy}
                    onClick={() => onSelectSeries(item.id)}
                  >
                    <span className={"nf-poster-frame artwork-" + item.artwork} aria-hidden="true">
                      <span className="nf-poster-fallback-title">{item.title}</span>
                      <CoverImage url={item.coverUrl} seriesId={item.id} eager={idx < 10} priority={idx < 3} />
                      <span className="nf-poster-gradient" />
                      <span className="nf-poster-badge">
                        {item.episodeCount > 1 ? `全 ${item.episodeCount} 集` : item.genre || "收藏"}
                      </span>
                      <span
                        className="nf-poster-hover-play"
                        onClick={e => {
                          e.stopPropagation();
                          onQuickPlaySeries(item);
                        }}
                      >
                        <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                      </span>
                    </span>
                    <span className="nf-poster-info">
                      <strong title={item.title}>{item.title}</strong>
                      <small>
                        {[item.genre, formatAddedAt(item.addedAtMs) ? `收藏于 ${formatAddedAt(item.addedAtMs)}` : ""]
                          .filter(Boolean)
                          .join(" · ")}
                      </small>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="nf-poster-fav-btn is-favorited"
                    disabled={busy}
                    aria-label={`取消收藏 ${item.title}`}
                    title="取消收藏"
                    onClick={e => {
                      e.stopPropagation();
                      void onRemoveFavorite(item.id);
                    }}
                  >
                    ★
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      {tab !== "series" && (
        <section className="nf-fav-section" aria-label="收藏的本地视频">
          {tab === "all" && (
            <div className="nf-section-divider">
              <h2>本地视频收藏</h2>
            </div>
          )}
          <LocalLibraryView
            key="favorites-local"
            favoritesOnly
            refreshSignal={refreshSignal}
            onPlay={onPlayLocal}
          />
        </section>
      )}
    </div>
  );
}
