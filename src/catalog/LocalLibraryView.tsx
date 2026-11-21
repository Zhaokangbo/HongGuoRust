import { useEffect, useState } from "react";
import { addLocalMedia, clearLocalHistory, listLocalMedia, removeLocalMedia, setLocalFavorite } from "./api";
import type { LocalMedia } from "./types";

interface Props {
  onPlay: (id: number) => Promise<boolean>;
  playerError: string;
  historyOnly?: boolean;
  favoritesOnly?: boolean;
}

function clock(sec: number): string {
  const whole = Math.max(0, Math.floor(sec));
  return `${Math.floor(whole / 60).toString().padStart(2, "0")}:${(whole % 60).toString().padStart(2, "0")}`;
}

export function LocalLibraryView({ onPlay, playerError, historyOnly = false, favoritesOnly = false }: Props) {
  const [items, setItems] = useState<LocalMedia[]>([]);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [confirmId, setConfirmId] = useState<number | null>(null);
  const [confirmClear, setConfirmClear] = useState<number | null>(null);

  useEffect(() => {
    let active = true;
    void listLocalMedia().then(result => {
      if (active) setItems(result);
    }).catch(reason => {
      if (active) setError(String(reason));
    }).finally(() => {
      if (active) setLoading(false);
    });
    return () => { active = false; };
  }, []);

  async function add() {
    setWorking(true);
    setError("");
    try {
      const selected = await addLocalMedia();
      if (selected) setItems(await listLocalMedia());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  async function remove(id: number) {
    setWorking(true);
    setError("");
    try {
      await removeLocalMedia(id);
      setItems(await listLocalMedia());
      setConfirmId(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  async function clear(id: number) {
    setWorking(true);
    setError("");
    try {
      await clearLocalHistory(id);
      setItems(await listLocalMedia());
      setConfirmClear(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  async function favorite(id: number, selected: boolean) {
    setWorking(true);
    setError("");
    try {
      await setLocalFavorite(id, selected);
      setItems(await listLocalMedia());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking(false);
    }
  }

  async function play(id: number) {
    setWorking(true);
    setError("");
    try {
      await onPlay(id);
    } finally {
      setWorking(false);
    }
  }

  const visible = historyOnly
    ? [...items].filter(item => item.lastPlayedMs !== null)
        .sort((a, b) => (b.lastPlayedMs ?? 0) - (a.lastPlayedMs ?? 0))
    : favoritesOnly ? items.filter(item => item.favorite) : items;

  return <div className="local-library">
    {!historyOnly && !favoritesOnly && <div className="local-library-toolbar">
      <button type="button" disabled={working} onClick={() => void add()}>+ 添加本地 MP4</button>
      <span>仅保存媒体文件位置，不复制、不上传视频文件。</span>
    </div>}
    {(error || playerError) && <p role="alert" className="catalog-error">{error || playerError}</p>}
    {loading ? <p className="catalog-empty" role="status">正在读取本地媒体目录…</p>
      : visible.length === 0 ? <p className="catalog-empty">{historyOnly
          ? "还没有观看记录。请到“我的视频”选择一部 MP4 播放。"
          : favoritesOnly ? "还没有收藏的视频。请到“我的视频”中点击“收藏”。"
          : "媒体库还是空的，点击“添加本地 MP4”开始。"}</p>
      : <div className="local-library-list">
        {visible.map(item => <div className="local-media-row" key={item.id}>
          <div className="local-media-icon" aria-hidden="true">MP4</div>
          <div className="local-media-summary">
            <strong title={item.filename}>{item.title}</strong>
            <span>{item.filename}</span>
            {item.lastPlayedMs !== null && <small className="local-history-detail">
              {item.completed ? "已看完" : `看到 ${clock(item.positionSec)}`}
              {item.durationSec !== null ? ` / ${clock(item.durationSec)}` : ""}
              {" · "}{new Date(item.lastPlayedMs).toLocaleString()}
            </small>}
            {item.lastPlayedMs !== null && item.durationSec !== null && item.durationSec > 0 &&
              <div className="local-progress" role="progressbar"
                aria-label={item.title + " 播放进度"} aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={item.completed ? 100 : Math.round(Math.min(100, Math.max(0, item.positionSec / item.durationSec * 100)))}>
                <div className="local-progress-fill" style={{ width: (item.completed ? 100 :
                  Math.min(100, Math.max(0, item.positionSec / item.durationSec * 100))) + "%" }} />
              </div>}
            {!item.available && <small role="status">原始文件不存在或已移动</small>}
          </div>
          <button type="button" disabled={working || !item.available}
            onClick={() => void play(item.id)}>{historyOnly && !item.completed && item.positionSec >= 5 ? "继续播放" : "播放"}</button>
          <button type="button" className="catalog-ghost" disabled={working}
            aria-pressed={item.favorite} onClick={() => void favorite(item.id, !item.favorite)}>
            {item.favorite ? "★ 已收藏" : "☆ 收藏"}
          </button>
          {historyOnly && (confirmClear === item.id ? <div className="local-media-confirm">
            <span>清除该条观看记录？</span>
            <button type="button" disabled={working} onClick={() => void clear(item.id)}>确认清除</button>
            <button type="button" className="catalog-ghost" disabled={working}
              onClick={() => setConfirmClear(null)}>取消</button>
          </div> : <button type="button" className="catalog-ghost" disabled={working}
            onClick={() => setConfirmClear(item.id)}>清除记录</button>)}
          {!historyOnly && !favoritesOnly && (confirmId === item.id ? <div className="local-media-confirm">
            <span>只从列表移除？</span>
            <button type="button" disabled={working}
              onClick={() => void remove(item.id)}>确认移除</button>
            <button type="button" className="catalog-ghost" disabled={working}
              onClick={() => setConfirmId(null)}>取消</button>
          </div> : <button type="button" className="catalog-ghost" disabled={working}
            onClick={() => setConfirmId(item.id)}>移除</button>)}
        </div>)}
      </div>}
    <p className="catalog-disclaimer">{historyOnly
      ? "观看进度只保存在本机。清除记录不会移除视频，也不会删除视频文件。"
      : favoritesOnly ? "取消收藏不会删除原始视频，也不会清除观看进度。"
      : "移除条目不会删除磁盘上的原始视频。文件被移动或删除后，旧条目会显示不可播放。"}</p>
  </div>;
}
