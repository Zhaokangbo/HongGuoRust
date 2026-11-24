import { useMemo, useState } from "react";
import type { FavoriteSeries, Series } from "./types";
import type { AccountStatusSnapshot, CloudLibrarySnapshot } from "../account/api";
import { CoverImage } from "./CoverImage";
import { LocalLibraryView } from "./LocalLibraryView";
import { isVisibleProvider } from "./discovery";

interface Props {
  favorites: FavoriteSeries[];
  busy: boolean;
  refreshSignal: number;
  accountStatus: AccountStatusSnapshot | null;
  cloudLibrary: CloudLibrarySnapshot | null;
  accountSyncing: boolean;
  onOpenLogin: () => void;
  onSyncAccount: () => Promise<void>;
  onSelectSeries: (id: string) => void;
  onQuickPlaySeries: (series: Series) => void;
  onRemoveFavorite: (seriesId: string) => Promise<void>;
  onPlayLocal: (id: number, orderedIds: number[], origin: string, titles?: Record<string, string>) => Promise<boolean>;
  onGoHome: () => void;
}

type FavTab = "all" | "series" | "reservations" | "local";
type FavSort = "recent" | "title";

function formatAddedAt(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit",
  }).format(new Date(ms));
}

function formatSyncTime(ms: number | null): string {
  if (!ms || !Number.isFinite(ms)) return "尚未同步";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit",
  }).format(new Date(ms));
}

export function FavoritesView({
  favorites, busy, refreshSignal, accountStatus, cloudLibrary, accountSyncing,
  onOpenLogin, onSyncAccount, onSelectSeries, onQuickPlaySeries,
  onRemoveFavorite, onPlayLocal, onGoHome,
}: Props) {
  const [tab, setTab] = useState<FavTab>("all");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<FavSort>("recent");

  const onlineReservations = cloudLibrary?.onlineReservations ?? [];
  const upcomingReservations = cloudLibrary?.upcomingReservations ?? [];
  const totalReservations = onlineReservations.length + upcomingReservations.length;

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
          <p className="nf-page-subtitle">集中管理你收藏的剧集、App 预约片单与本地视频，随时接着看</p>
        </div>
        <div className="nf-chip-group" role="tablist" aria-label="收藏分类">
          <button
            type="button"
            role="tab"
            aria-selected={tab === "all"}
            className={tab === "all" ? "is-active" : ""}
            onClick={() => setTab("all")}
          >
            全部
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
          {accountStatus?.signedIn && (
            <button
              type="button"
              role="tab"
              aria-selected={tab === "reservations"}
              className={tab === "reservations" ? "is-active" : ""}
              onClick={() => setTab("reservations")}
            >
              App 预约 ({totalReservations})
            </button>
          )}
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

      {/* Cloud Account Sync Banner */}
      <div className="nf-account-banner">
        {accountStatus?.signedIn ? (
          <>
            <div className="nf-account-banner-info">
              <span className="nf-account-dot is-online" aria-hidden="true" />
              <div>
                <strong>{accountStatus.nickname || "红果用户"} · 云端已连接</strong>
                <span>
                  云端追剧 {accountStatus.favoritesCount} 部 · 历史 {accountStatus.historyCount} 部 · 预约 {accountStatus.reservationsCount} 部 · 上次同步 {formatSyncTime(accountStatus.lastSyncedMs)}
                </span>
              </div>
            </div>
            <button
              type="button"
              className="nf-btn-secondary"
              disabled={accountSyncing}
              onClick={() => void onSyncAccount()}
            >
              {accountSyncing ? "正在同步…" : "立即同步"}
            </button>
          </>
        ) : (
          <>
            <div className="nf-account-banner-info">
              <span className="nf-account-dot" aria-hidden="true" />
              <div>
                <strong>扫码同步红果 App 片单</strong>
                <span>登录后自动同步手机端「我的追剧」、观看进度和预约片单，支持双向收藏与进度回传</span>
              </div>
            </div>
            <button
              type="button"
              className="nf-btn-play"
              onClick={onOpenLogin}
            >
              扫码登录
            </button>
          </>
        )}
      </div>

      {(tab === "all" || tab === "series") && (
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

      {accountStatus?.signedIn && (tab === "reservations" || (tab === "all" && totalReservations > 0)) && (
        <section className="nf-fav-section" aria-label="App 预约片单">
          <div className="nf-fav-toolbar">
            <div className="nf-fav-section-title">
              <h2>App 预约片单</h2>
              <span>{totalReservations} 部</span>
            </div>
          </div>

          {totalReservations === 0 ? (
            <div className="nf-empty-panel">
              <h3>暂无预约短剧</h3>
              <p>在手机红果 App 预约的新剧会自动同步到这里，上线后可直接点击播放。</p>
            </div>
          ) : (
            <>
              {onlineReservations.length > 0 && (
                <div className="nf-res-group">
                  <h3 className="nf-res-subtitle">已上线 · 可直接观看 ({onlineReservations.length})</h3>
                  <div className="nf-poster-grid">
                    {onlineReservations.map((item, idx) => (
                      <div key={item.series.id} className="nf-poster-card-wrap">
                        <button
                          type="button"
                          className="nf-poster-card"
                          disabled={busy}
                          onClick={() => onSelectSeries(item.series.id)}
                        >
                          <span className={"nf-poster-frame artwork-" + item.series.artwork} aria-hidden="true">
                            <span className="nf-poster-fallback-title">{item.series.title}</span>
                            <CoverImage url={item.series.coverUrl} seriesId={item.series.id} eager={idx < 8} />
                            <span className="nf-poster-gradient" />
                            <span className="nf-poster-badge nf-reservation-badge-online">
                              {item.series.episodeCount > 0 ? `已上线 · 全 ${item.series.episodeCount} 集` : "已上线"}
                            </span>
                            <span
                              className="nf-poster-hover-play"
                              onClick={e => {
                                e.stopPropagation();
                                onQuickPlaySeries(item.series);
                              }}
                            >
                              <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                            </span>
                          </span>
                          <span className="nf-poster-info">
                            <strong title={item.series.title}>{item.series.title}</strong>
                            <small>{item.publishText || item.series.genre || "已上线预约"}</small>
                          </span>
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {upcomingReservations.length > 0 && (
                <div className="nf-res-group">
                  <h3 className="nf-res-subtitle">待上线 ({upcomingReservations.length})</h3>
                  <div className="nf-poster-grid">
                    {upcomingReservations.map((item, idx) => (
                      <div key={item.series.id} className="nf-poster-card-wrap">
                        <div className="nf-poster-card is-upcoming" aria-disabled="true">
                          <span className={"nf-poster-frame artwork-" + item.series.artwork} aria-hidden="true">
                            <span className="nf-poster-fallback-title">{item.series.title}</span>
                            <CoverImage url={item.series.coverUrl} seriesId={item.series.id} eager={idx < 8} />
                            <span className="nf-poster-gradient" />
                            <span className="nf-poster-badge nf-reservation-badge-upcoming">
                              待上线
                            </span>
                          </span>
                          <span className="nf-poster-info">
                            <strong title={item.series.title}>{item.series.title}</strong>
                            <small>{item.publishText || "敬请期待"}</small>
                          </span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}
        </section>
      )}

      {(tab === "all" || tab === "local") && (
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
