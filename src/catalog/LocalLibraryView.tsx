import { useEffect, useRef, useState } from "react";
import {
  addLocalMedia, clearLocalHistory, listLocalMedia, removeLocalMedia, setLocalFavorite,
} from "./api";
import type { LocalMedia } from "./types";
import { CoverImage } from "./CoverImage";
import {
  browseLocalMedia, playlistFromVisible,
  type LibraryFilter, type LibraryMode, type LibrarySort,
} from "./localBrowse";

interface Props {
  onPlay: (id: number, orderedIds: number[], origin: string, titles?: Record<string, string>) => Promise<boolean>;
  historyOnly?: boolean;
  favoritesOnly?: boolean;
  refreshSignal?: number;
}

function formatClock(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "--:--";
  const whole = Math.max(0, Math.floor(value));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const seconds = whole % 60;
  return hours > 0
    ? [hours, minutes, seconds].map(part => part.toString().padStart(2, "0")).join(":")
    : [minutes, seconds].map(part => part.toString().padStart(2, "0")).join(":");
}

function playedAt(value: number | null): string {
  if (value === null || !Number.isFinite(value) || value <= 0 || value > 8.64e15) {
    return "尚未观看";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit",
  }).format(new Date(value));
}

function progressPercent(item: LocalMedia): number | null {
  if (item.lastPlayedMs === null) return null;
  if (item.completed) return 100;
  if (item.durationSec === null || !Number.isFinite(item.durationSec) || item.durationSec <= 0) {
    return null;
  }
  const position = Number.isFinite(item.positionSec) ? Math.max(0, item.positionSec) : 0;
  return Math.round(Math.min(100, Math.max(0, (position / item.durationSec) * 100)));
}

export function LocalLibraryView({
  onPlay, historyOnly = false, favoritesOnly = false, refreshSignal = 0,
}: Props) {
  const mode: LibraryMode = historyOnly ? "history" : favoritesOnly ? "favorites" : "local";
  const origin = historyOnly ? "观看历史" : favoritesOnly ? "我的片单" : "本地片库";
  const [items, setItems] = useState<LocalMedia[]>([]);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [confirmRemoveId, setConfirmRemoveId] = useState<number | null>(null);
  const [confirmClearId, setConfirmClearId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<LibraryFilter>("all");
  const [sort, setSort] = useState<LibrarySort>(historyOnly ? "recent" : "added");
  const loadVersion = useRef(0);

  async function reload(showLoading = false) {
    const version = ++loadVersion.current;
    if (showLoading) setLoading(true);
    try {
      const result = await listLocalMedia();
      if (version === loadVersion.current) {
        setItems(result);
        setError("");
      }
    } catch {
      if (version === loadVersion.current) setError("读取本地片库失败，请重试。");
    } finally {
      if (version === loadVersion.current) setLoading(false);
    }
  }

  useEffect(() => {
    void reload(true);
    return () => { loadVersion.current++; };
  }, [refreshSignal]);

  async function mutate(action: () => Promise<void>, failureMessage: string, after?: () => void) {
    if (working) return;
    setWorking(true);
    setError("");
    try {
      await action();
      await reload(false);
      after?.();
    } catch {
      setError(failureMessage);
    } finally {
      setWorking(false);
    }
  }

  async function add() {
    if (working) return;
    setWorking(true);
    setError("");
    try {
      const selected = await addLocalMedia();
      if (selected) await reload(false);
    } catch {
      setError("添加本地视频失败，请确认文件可访问且为 MP4 格式。");
    } finally {
      setWorking(false);
    }
  }

  async function play(id: number) {
    if (working) return;
    setWorking(true);
    setError("");
    try {
      const titles: Record<string, string> = {};
      for (const item of visible) titles[String(item.id)] = item.title;
      await onPlay(id, playlistFromVisible(visible), origin, titles);
    } catch {
      setError("无法播放该视频，请确认原文件仍存在。");
    } finally {
      setWorking(false);
    }
  }

  const { total, visible } = browseLocalMedia(items, mode, query, sort, filter);
  const hasActiveFilters = query.trim().length > 0 || filter !== "all";
  const filters: { value: LibraryFilter; label: string }[] = historyOnly
    ? [
      { value: "all", label: "全部记录" },
      { value: "in-progress", label: "观看中" },
      { value: "completed", label: "已看完" },
      { value: "available", label: "可用" },
      { value: "missing", label: "文件缺失" },
    ]
    : [
      { value: "all", label: "全部" },
      { value: "in-progress", label: "观看中" },
      { value: "completed", label: "已看完" },
      { value: "unplayed", label: "未观看" },
      { value: "missing", label: "文件缺失" },
    ];

  const emptyTitle = hasActiveFilters ? "没有找到匹配的视频"
    : historyOnly ? "还没有本地观看记录"
      : favoritesOnly ? "我的片单还是空的" : "本地片库还是空的";

  const emptyDesc = hasActiveFilters
    ? "试试其他关键词或清除筛选条件。"
    : historyOnly ? "播放本地视频后会自动记录观看进度。"
      : favoritesOnly ? "在本地片库点击收藏，喜欢的影片会集中展示在这里。"
        : "添加电脑上的 MP4 视频即可开始整理与播放。";

  return (
    <section className="nf-library" aria-label={origin}>
      <div className="nf-library-bar">
        <div className="nf-library-filters">
          <input
            type="search"
            className="nf-input-dark"
            aria-label={`搜索${origin}`}
            placeholder="搜索标题或文件名…"
            maxLength={128}
            value={query}
            onChange={e => setQuery(e.target.value)}
          />
          <select
            className="nf-select-dark"
            aria-label="状态筛选"
            value={filter}
            onChange={e => setFilter(e.target.value as LibraryFilter)}
          >
            {filters.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
          </select>
          <select
            className="nf-select-dark"
            aria-label="排序方式"
            value={sort}
            onChange={e => setSort(e.target.value as LibrarySort)}
          >
            <option value="added">按添加顺序</option>
            <option value="recent">最近播放</option>
            <option value="title">按标题排序</option>
            <option value="progress">按观看进度</option>
          </select>
          {hasActiveFilters && (
            <button type="button" className="nf-btn-ghost"
              onClick={() => { setQuery(""); setFilter("all"); }}>
              清除筛选
            </button>
          )}
        </div>
        <div className="nf-library-actions">
          <span className="nf-count-label">{visible.length} / {total} 部</span>
          {!historyOnly && !favoritesOnly && (
            <button type="button" className="nf-btn-red" disabled={working} onClick={() => void add()}>
              ＋ 添加本地 MP4
            </button>
          )}
        </div>
      </div>

      {error && (
        <div className="nf-alert-banner" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => void reload(true)}>重试</button>
        </div>
      )}

      {loading ? (
        <div className="nf-rail-skeleton" role="status"><span /><span /><span /><span /></div>
      ) : visible.length === 0 ? (
        <div className="nf-empty-panel">
          <h3>{emptyTitle}</h3>
          <p>{emptyDesc}</p>
          {hasActiveFilters ? (
            <button type="button" className="nf-btn-secondary"
              onClick={() => { setQuery(""); setFilter("all"); }}>清除筛选</button>
          ) : (!historyOnly && !favoritesOnly && (
            <button type="button" className="nf-btn-play" disabled={working} onClick={() => void add()}>
              ＋ 添加第一部视频
            </button>
          ))}
        </div>
      ) : (
        <div className="nf-local-grid">
          {visible.map(item => {
            const percent = progressPercent(item);
            const resume = item.lastPlayedMs !== null && !item.completed && item.positionSec >= 5;
            const playLabel = resume ? "继续播放" : item.completed ? "重看" : "播放";
            return (
              <article key={item.id} className={"nf-local-card" + (!item.available ? " is-missing" : "")}>
                <button
                  type="button"
                  className="nf-local-thumb"
                  disabled={working || !item.available}
                  aria-label={`${playLabel} ${item.title}`}
                  onClick={() => void play(item.id)}
                >
                  <span className="nf-local-thumb-bg" aria-hidden="true">
                    <CoverImage localId={item.id} />
                    <span className="nf-local-thumb-shade" />
                    <span className="nf-local-ext">MP4</span>
                    <span className="nf-local-play-circle">
                      <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                    </span>
                  </span>
                  {item.favorite && <span className="nf-local-fav-pill">★ 已收藏</span>}
                  {!item.available && <span className="nf-local-miss-pill">文件不可用</span>}
                  {percent !== null && (
                    <span className="nf-card-progress" role="progressbar"
                      aria-label={`${item.title} 进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
                      <i style={{ width: `${percent}%` }} />
                    </span>
                  )}
                </button>
                <div className="nf-local-body">
                  <strong title={item.title}>{item.title}</strong>
                  <div className="nf-local-meta">
                    <span>{playedAt(item.lastPlayedMs)}</span>
                    <span>{item.durationSec !== null ? formatClock(item.durationSec) : "MP4"}</span>
                  </div>
                  <div className="nf-local-card-actions">
                    <button
                      type="button"
                      className="nf-local-btn-play"
                      disabled={working || !item.available}
                      onClick={() => void play(item.id)}
                    >
                      {playLabel}
                    </button>
                    <button
                      type="button"
                      className={"nf-local-btn-icon" + (item.favorite ? " is-fav" : "")}
                      disabled={working}
                      aria-pressed={item.favorite}
                      title={item.favorite ? "取消收藏" : "加入片单"}
                      onClick={() => void mutate(
                        () => setLocalFavorite(item.id, !item.favorite),
                        "更新收藏状态失败。",
                      )}
                    >
                      {item.favorite ? "★" : "☆"}
                    </button>
                    {historyOnly ? (
                      confirmClearId === item.id ? (
                        <button type="button" className="nf-local-btn-danger" disabled={working}
                          onClick={() => void mutate(() => clearLocalHistory(item.id), "清除记录失败。", () => setConfirmClearId(null))}>
                          确认清除
                        </button>
                      ) : (
                        <button type="button" className="nf-local-btn-text" disabled={working}
                          onClick={() => setConfirmClearId(item.id)}>清除</button>
                      )
                    ) : !favoritesOnly && (
                      confirmRemoveId === item.id ? (
                        <button type="button" className="nf-local-btn-danger" disabled={working}
                          onClick={() => void mutate(() => removeLocalMedia(item.id), "移除失败。", () => setConfirmRemoveId(null))}>
                          确认移除
                        </button>
                      ) : (
                        <button type="button" className="nf-local-btn-text" disabled={working}
                          onClick={() => setConfirmRemoveId(item.id)}>移除</button>
                      )
                    )}
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
