import { useEffect, useRef, useState } from "react";
import {
  addLocalMedia, clearLocalHistory, listLocalMedia, removeLocalMedia, setLocalFavorite,
} from "./api";
import type { LocalMedia } from "./types";
import {
  browseLocalMedia, localLibrarySummary, playlistFromVisible,
  type LibraryFilter, type LibraryMode, type LibrarySort,
} from "./localBrowse";

interface Props {
  onPlay: (id: number, orderedIds: number[], origin: string) => Promise<boolean>;
  playerError: string;
  historyOnly?: boolean;
  favoritesOnly?: boolean;
}

function clock(value: number | null): string {
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
    return "尚未播放";
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
  return Math.round(Math.min(100, Math.max(0, position / item.durationSec * 100)));
}

export function LocalLibraryView({
  onPlay, playerError, historyOnly = false, favoritesOnly = false,
}: Props) {
  const mode: LibraryMode = historyOnly ? "history" : favoritesOnly ? "favorites" : "local";
  const origin = historyOnly ? "最近播放" : favoritesOnly ? "我的收藏" : "我的视频";
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
    } catch (reason) {
      if (version === loadVersion.current) setError(String(reason));
    } finally {
      if (version === loadVersion.current) setLoading(false);
    }
  }

  useEffect(() => {
    void reload(true);
    return () => { loadVersion.current++; };
  }, []);

  async function mutate(action: () => Promise<void>, after?: () => void) {
    if (working) return;
    setWorking(true);
    setError("");
    try {
      await action();
      await reload(false);
      after?.();
    } catch (reason) {
      setError(String(reason));
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
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  async function play(id: number) {
    if (working) return;
    setWorking(true);
    setError("");
    try {
      await onPlay(id, playlistFromVisible(visible), origin);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  const { total, matched, visible } = browseLocalMedia(items, mode, query, sort, filter);
  const summary = localLibrarySummary(items, mode);
  const hasActiveFilters = query.trim().length > 0 || filter !== "all";
  const filters: { value: LibraryFilter; label: string }[] = historyOnly
    ? [
      { value: "all", label: "全部记录" },
      { value: "in-progress", label: "观看中" },
      { value: "completed", label: "已看完" },
      { value: "available", label: "文件可用" },
      { value: "missing", label: "文件不可用" },
    ]
    : [
      { value: "all", label: "全部视频" },
      { value: "available", label: "文件可用" },
      { value: "missing", label: "文件不可用" },
      { value: "in-progress", label: "观看中" },
      { value: "completed", label: "已看完" },
      { value: "unplayed", label: "尚未播放" },
    ];
  const stats = historyOnly
    ? [
      { label: "观看记录", value: summary.total, note: "本机保存" },
      { label: "观看中", value: summary.inProgress, note: "可继续播放" },
      { label: "已看完", value: summary.completed, note: "已完成" },
      { label: "文件不可用", value: summary.missing, note: "需检查原文件" },
    ]
    : favoritesOnly
      ? [
        { label: "收藏视频", value: summary.total, note: "本机收藏" },
        { label: "文件可用", value: summary.available, note: "可以播放" },
        { label: "观看中", value: summary.inProgress, note: "保留播放进度" },
        { label: "文件不可用", value: summary.missing, note: "收藏仍保留" },
      ]
      : [
        { label: "媒体总数", value: summary.total, note: "已添加到本机" },
        { label: "文件可用", value: summary.available, note: "可以播放" },
        { label: "观看中", value: summary.inProgress, note: "有播放记录" },
        { label: "我的收藏", value: summary.favorites, note: "随时继续观看" },
      ];

  const emptyTitle = hasActiveFilters ? "没有找到匹配的视频"
    : historyOnly ? "还没有观看记录"
      : favoritesOnly ? "收藏夹还是空的" : "媒体库还是空的";
  const emptyDescription = hasActiveFilters
    ? "试试其他关键词或状态筛选，也可以清除筛选条件。"
    : historyOnly ? "播放过的本地视频会自动出现在这里，观看进度保存在本机。"
      : favoritesOnly ? "在“我的视频”中点击收藏，喜欢的本地视频就会集中显示在这里。"
        : "添加你自己的 MP4 文件。应用只记录文件位置，不会复制或上传视频。";

  return <section className="local-library personal-library" aria-label={origin}>
    <div className="personal-library-overview" aria-label="媒体统计">
      {stats.map(stat => <div className="personal-library-stat" key={stat.label}>
        <span>{stat.label}</span>
        <strong>{stat.value}</strong>
        <small>{stat.note}</small>
      </div>)}
    </div>

    <div className="personal-library-toolbar">
      <div className="personal-library-toolbar-copy">
        <strong>{historyOnly ? "播放足迹" : favoritesOnly ? "收藏清单" : "本地片库"}</strong>
        <span>{historyOnly ? "按最近播放时间整理，保留每部视频的观看进度。"
          : favoritesOnly ? "只管理收藏状态，不改变文件和观看记录。"
            : "把自己的视频集中整理，按需搜索、筛选和播放。"}</span>
      </div>
      <div className="personal-library-toolbar-actions">
        {!historyOnly && !favoritesOnly && <button type="button"
          className="personal-library-add" disabled={working} onClick={() => void add()}>
          <span aria-hidden="true">＋</span> 添加本地 MP4
        </button>}
        <button type="button" className="personal-library-refresh"
          disabled={working || loading} onClick={() => void reload(true)}>
          {loading ? "读取中…" : "刷新列表"}
        </button>
      </div>
    </div>

    <div className="personal-library-controls">
      <label className="personal-library-search">
        <span>搜索媒体</span>
        <input type="search" aria-label="搜索本地视频" placeholder="输入标题或文件名"
          maxLength={128} value={query} onChange={event => setQuery(event.target.value)} />
      </label>
      <label>
        <span>状态筛选</span>
        <select aria-label="媒体状态筛选" value={filter}
          onChange={event => setFilter(event.target.value as LibraryFilter)}>
          {filters.map(option => <option value={option.value} key={option.value}>{option.label}</option>)}
        </select>
      </label>
      <label>
        <span>排序方式</span>
        <select aria-label="媒体排序方式" value={sort}
          onChange={event => setSort(event.target.value as LibrarySort)}>
          <option value="added">最近添加</option>
          <option value="recent">最近播放</option>
          <option value="title">标题（自然排序）</option>
          <option value="progress">观看进度</option>
        </select>
      </label>
      <div className="personal-library-result-count" role="status">
        <strong>{visible.length}</strong><span> / {matched} 条</span>
        <small>共 {total} 条</small>
      </div>
      {hasActiveFilters && <button type="button" className="personal-library-reset"
        onClick={() => { setQuery(""); setFilter("all"); }}>清除筛选</button>}
    </div>

    {(error || playerError) && <div className="personal-library-error" role="alert">
      <span>{error || playerError}</span>
      {error && <button type="button" onClick={() => void reload(true)}>重试</button>}
    </div>}

    {loading ? <div className="personal-library-state" role="status">
      <span className="personal-library-state-icon" aria-hidden="true">◌</span>
      <strong>正在读取媒体库</strong>
      <p>正在检查本机保存的媒体记录…</p>
    </div> : visible.length === 0 ? <div className="personal-library-state">
      <span className="personal-library-state-icon" aria-hidden="true">
        {historyOnly ? "◷" : favoritesOnly ? "☆" : "▣"}
      </span>
      <strong>{emptyTitle}</strong>
      <p>{emptyDescription}</p>
      {hasActiveFilters && <button type="button" className="personal-library-reset"
        onClick={() => { setQuery(""); setFilter("all"); }}>清除筛选条件</button>}
      {!hasActiveFilters && !historyOnly && !favoritesOnly && <button type="button"
        className="personal-library-add" onClick={() => void add()} disabled={working}>
        添加第一部视频
      </button>}
    </div> : <div className="personal-media-grid">
      {visible.map(item => {
        const percent = progressPercent(item);
        const resume = item.lastPlayedMs !== null && !item.completed && item.positionSec >= 5;
        const status = !item.available ? "文件不可用"
          : item.completed ? "已看完"
            : item.lastPlayedMs !== null ? "观看中" : "未播放";
        const favoriteLabel = favoritesOnly ? "取消收藏"
          : item.favorite ? "已收藏" : "收藏";
        return <article className={"personal-media-card" + (!item.available ? " is-missing" : "")}
          key={item.id}>
          <div className="personal-media-art" aria-hidden="true">
            <span className="personal-media-art-kicker">HONGGUORUST · LOCAL</span>
            <span className="personal-media-art-symbol">▶</span>
            <span className="personal-media-art-format">MP4</span>
            {item.favorite && <span className="personal-media-art-favorite">★ 已收藏</span>}
            {!item.available && <span className="personal-media-art-missing">文件已移动或删除</span>}
          </div>
          <div className="personal-media-content">
            <div className="personal-media-title-row">
              <h3 title={item.title}>{item.title}</h3>
              <span className={"personal-media-status" + (!item.available ? " is-missing" : "")}>
                {status}
              </span>
            </div>
            <p className="personal-media-filename" title={item.filename}>{item.filename}</p>
            <div className="personal-media-meta">
              <span>{playedAt(item.lastPlayedMs)}</span>
              <span>{item.durationSec !== null ? clock(item.durationSec) : "时长待获取"}</span>
            </div>
            {percent !== null && <div className="personal-media-progress">
              <div className="personal-media-progress-track" role="progressbar"
                aria-label={item.title + " 播放进度"} aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={percent}>
                <span style={{ width: percent + "%" }} />
              </div>
              <small>{item.completed ? "已完成" : "已观看 " + clock(item.positionSec)}</small>
            </div>}
            <div className="personal-media-actions">
              <button type="button" className="personal-media-play" disabled={working || !item.available}
                onClick={() => void play(item.id)}>
                {resume ? "继续播放" : item.completed ? "重新播放" : "播放"}
              </button>
              <button type="button" className="personal-media-favorite" disabled={working}
                aria-pressed={item.favorite}
                aria-label={(item.favorite ? "取消收藏 " : "收藏 ") + item.title}
                onClick={() => void mutate(
                  () => setLocalFavorite(item.id, !item.favorite),
                )}>
                <span aria-hidden="true">{item.favorite ? "★" : "☆"}</span>
                {favoriteLabel}
              </button>
            </div>
            {historyOnly ? confirmClearId === item.id
              ? <div className="personal-media-confirm" role="group" aria-label="清除观看记录确认">
                  <span>仅清除记录，不删除视频。</span>
                  <button type="button" disabled={working}
                    onClick={() => void mutate(() => clearLocalHistory(item.id), () => setConfirmClearId(null))}>
                    确认清除
                  </button>
                  <button type="button" className="is-secondary" disabled={working}
                    onClick={() => setConfirmClearId(null)}>取消</button>
                </div>
              : <button type="button" className="personal-media-text-action" disabled={working}
                  onClick={() => setConfirmClearId(item.id)}>清除记录</button>
              : !favoritesOnly && (confirmRemoveId === item.id
                ? <div className="personal-media-confirm" role="group" aria-label="移除媒体确认">
                    <span>仅从媒体库移除，不删除原文件。</span>
                    <button type="button" disabled={working}
                      onClick={() => void mutate(() => removeLocalMedia(item.id), () => setConfirmRemoveId(null))}>
                      确认移除
                    </button>
                    <button type="button" className="is-secondary" disabled={working}
                      onClick={() => setConfirmRemoveId(null)}>取消</button>
                  </div>
                : <button type="button" className="personal-media-text-action" disabled={working}
                    onClick={() => setConfirmRemoveId(item.id)}>移出媒体库</button>)}
          </div>
        </article>;
      })}
    </div>}

    <p className="personal-library-disclaimer">{historyOnly
      ? "观看进度保存在本机。清除记录不会移除媒体库条目、收藏状态或原始视频文件。"
      : favoritesOnly ? "取消收藏不会清除观看记录，也不会删除磁盘上的原始视频。"
        : "本地媒体只保存文件位置。移出媒体库不会删除磁盘上的原始视频文件。"}</p>
  </section>;
}
