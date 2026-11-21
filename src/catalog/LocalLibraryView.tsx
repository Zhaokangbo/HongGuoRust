import { useEffect, useState } from "react";
import { addLocalMedia, listLocalMedia, removeLocalMedia } from "./api";
import type { LocalMedia } from "./types";

interface Props {
  onPlay: (id: number) => Promise<boolean>;
  playerError: string;
}

export function LocalLibraryView({ onPlay, playerError }: Props) {
  const [items, setItems] = useState<LocalMedia[]>([]);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [confirmId, setConfirmId] = useState<number | null>(null);

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

  async function play(id: number) {
    setWorking(true);
    setError("");
    try {
      await onPlay(id);
    } finally {
      setWorking(false);
    }
  }

  return <div className="local-library">
    <div className="local-library-toolbar">
      <button type="button" disabled={working} onClick={() => void add()}>+ 添加本地 MP4</button>
      <span>仅保存媒体文件位置，不复制、不上传视频文件。</span>
    </div>
    {(error || playerError) && <p role="alert" className="catalog-error">{error || playerError}</p>}
    {loading ? <p className="catalog-empty" role="status">正在读取本地媒体目录…</p>
      : items.length === 0 ? <p className="catalog-empty">媒体库还是空的，点击“添加本地 MP4”开始。</p>
      : <div className="local-library-list">
        {items.map(item => <div className="local-media-row" key={item.id}>
          <div className="local-media-icon" aria-hidden="true">MP4</div>
          <div className="local-media-summary">
            <strong title={item.filename}>{item.title}</strong>
            <span>{item.filename}</span>
            {!item.available && <small role="status">原始文件不存在或已移动</small>}
          </div>
          <button type="button" disabled={working || !item.available}
            onClick={() => void play(item.id)}>播放</button>
          {confirmId === item.id ? <div className="local-media-confirm">
            <span>只从列表移除？</span>
            <button type="button" disabled={working}
              onClick={() => void remove(item.id)}>确认移除</button>
            <button type="button" className="catalog-ghost" disabled={working}
              onClick={() => setConfirmId(null)}>取消</button>
          </div> : <button type="button" className="catalog-ghost" disabled={working}
            onClick={() => setConfirmId(item.id)}>移除</button>}
        </div>)}
      </div>}
    <p className="catalog-disclaimer">移除条目不会删除磁盘上的原始视频。文件被移动或删除后，旧条目会显示不可播放。</p>
  </div>;
}
