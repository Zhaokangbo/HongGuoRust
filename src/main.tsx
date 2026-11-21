import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import { nextAtNaturalEof, type LocalPlaylist } from "./player/autoNext";
import { CatalogView } from "./catalog/CatalogView";
import type { SeriesDetail } from "./catalog/types";
import {
  adjacentEpisode, createEpisodeQueue, currentEpisodeEnded, currentEpisodeFailed,
  type EpisodeQueue,
} from "./player/episodeQueue";
import "./style.css";

type PlayerCommand = "open_local" | "open_https" | "stop" | "set_paused" | "seek_to"
  | "set_rate" | "set_volume" | "set_fullscreen" | "set_catalog_visible" | "catalog_play_episode" | "library_play";
// Public sample source used for manual streaming checks. External URLs can change.
const SAMPLE = "https://samplelib.com/lib/preview/mp4/sample-20s.mp4";

function clock(value?: number): string {
  if (value === undefined || !Number.isFinite(value)) return "--:--";
  const seconds = Math.floor(Math.max(0, value));
  return `${Math.floor(seconds / 60).toString().padStart(2, "0")}:${(seconds % 60).toString().padStart(2, "0")}`;
}

function App() {
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [connectionError, setConnectionError] = useState("");
  const [commandError, setCommandError] = useState("");
  const [busy, setBusy] = useState(false);
  const [httpsUrl, setHttpsUrl] = useState(SAMPLE);
  const [seekText, setSeekText] = useState("0");
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const [rate, setRate] = useState(1);
  const [volume, setVolume] = useState(100);
  const [fullscreen, setFullscreen] = useState(false);
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [playlist, setPlaylist] = useState<LocalPlaylist | null>(null);
  // Separate from M8's local-file auto-next queue; Provider series are manual only.
  const [episodeQueue, setEpisodeQueue] = useState<EpisodeQueue | null>(null);
  const [autoNext, setAutoNext] = useState(false);
  const [autoNextError, setAutoNextError] = useState("");
  // A native session ID is the sole identity for an EOF event.
  const consumedEofRef = useRef<number | null>(null);
  const transitionRef = useRef(false);

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function refresh() {
      let delay = 250;
      try {
        const result = await getSnapshot();
        if (!active) return;
        setSnapshot(result);
        setConnectionError("");
      } catch (reason) {
        if (!active) return;
        setConnectionError(String(reason));
        delay = 750;
      }
      timer = setTimeout(refresh, delay);
    }
    void refresh();
    return () => { active = false; clearTimeout(timer); };
  }, []);

  useEffect(() => {
    let mounted = true;
    const timer = setInterval(() => {
      void window.__TAURI__?.core.invoke<boolean>("get_fullscreen").then(value => {
        if (mounted) setFullscreen(value);
      }).catch(() => { /* initialisation or closing: get_snapshot reports IPC errors */ });
    }, 800);
    return () => { mounted = false; clearInterval(timer); };
  }, []);

  useEffect(() => {
    setRate(1);
    setVolume(100);
    setSeekPreview(null);
    setSeekText("0");
  }, [snapshot?.sessionId]);

  async function command(name: PlayerCommand, args?: Record<string, unknown>): Promise<boolean> {
    setBusy(true);
    setCommandError("");
    try {
      if (!window.__TAURI__) throw new Error("Rust desktop bridge unavailable");
      await window.__TAURI__.core.invoke(name, args);
      return true;
    } catch (reason) {
      setCommandError(String(reason));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function toggleFullscreen(target: boolean) {
    const ok = await command("set_fullscreen", { fullscreen: target });
    if (ok) setFullscreen(target);
  }

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && fullscreen) {
        event.preventDefault();
        void toggleFullscreen(false);
      } else if (event.key === "F11") {
        event.preventDefault();
        void toggleFullscreen(!fullscreen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fullscreen]);

  async function openCatalog() {
    if (fullscreen && !(await command("set_fullscreen", { fullscreen: false }))) return;
    if (fullscreen) setFullscreen(false);
    if (await command("set_catalog_visible", { visible: true })) setCatalogOpen(true);
  }

  async function backToPlayer() {
    if (await command("set_catalog_visible", { visible: false })) setCatalogOpen(false);
  }

  /** Queue playback uses the session ID returned by the existing Rust IPC. */
  async function loadQueuedItem(ids: number[], index: number, origin: string): Promise<boolean> {
    if (transitionRef.current || index < 0 || index >= ids.length) return false;
    transitionRef.current = true;
    setBusy(true);
    setCommandError("");
    setAutoNextError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("Rust desktop bridge unavailable");
      const sessionId = await core.invoke<number>("library_play", { id: ids[index] });
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("播放接口未返回有效会话 ID");
      }
      setPlaylist({ ids: [...ids], index, origin, sessionId });
      setEpisodeQueue(null);
      return true;
    } catch (reason) {
      setCommandError(String(reason));
      return false;
    } finally {
      transitionRef.current = false;
      setBusy(false);
    }
  }

  async function openProviderEpisode(seriesId: string, episodeId: string): Promise<number | null> {
    if (transitionRef.current || busy) return null;
    transitionRef.current = true;
    setBusy(true);
    setCommandError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("Rust desktop bridge unavailable");
      const sessionId = await core.invoke<number>("catalog_play_episode", { seriesId, episodeId });
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("选集播放接口未返回有效会话 ID");
      }
      return sessionId;
    } catch (reason) {
      setCommandError(String(reason));
      return null;
    } finally {
      transitionRef.current = false;
      setBusy(false);
    }
  }

  async function playFromCatalog(
    seriesId: string, episodeId: string, detail: SeriesDetail,
  ): Promise<boolean> {
    if (detail.series.id !== seriesId || !detail.episodes.some(ep => ep.id === episodeId)) {
      setCommandError("当前选集与作品详情不匹配，请刷新详情后重试");
      return false;
    }
    // Build and validate navigation context before opening native media.
    try {
      createEpisodeQueue(detail, episodeId, 1);
    } catch (reason) {
      setCommandError(String(reason));
      return false;
    }
    const sessionId = await openProviderEpisode(seriesId, episodeId);
    if (sessionId === null) return false;
    consumedEofRef.current = playlist?.sessionId ?? null;
    setPlaylist(null);
    setAutoNextError("");
    setEpisodeQueue(createEpisodeQueue(detail, episodeId, sessionId));
    await backToPlayer();
    return true;
  }

  async function moveEpisode(delta: -1 | 1) {
    const selected = adjacentEpisode(episodeQueue, delta);
    if (!selected || !episodeQueue || busy || transitionRef.current) return;
    const previousSession = episodeQueue.sessionId;
    const sessionId = await openProviderEpisode(episodeQueue.seriesId, selected.episode.id);
    if (sessionId === null) return;
    setEpisodeQueue(current => current && current.sessionId === previousSession
      ? { ...current, index: selected.index, sessionId } : current);
  }

  async function retryEpisode() {
    if (!episodeQueue || busy || transitionRef.current) return;
    const oldSession = episodeQueue.sessionId;
    const sessionId = await openProviderEpisode(
      episodeQueue.seriesId, episodeQueue.episodes[episodeQueue.index].id,
    );
    if (sessionId !== null) {
      setEpisodeQueue(current => current && current.sessionId === oldSession
        ? { ...current, sessionId } : current);
    }
  }

  async function playFromLocal(id: number, orderedIds: number[], origin: string): Promise<boolean> {
    const index = orderedIds.indexOf(id);
    const ids = index < 0 ? [id] : orderedIds;
    const played = await loadQueuedItem(ids, index < 0 ? 0 : index, origin);
    if (played) await backToPlayer();
    return played;
  }

  async function movePlaylist(delta: -1 | 1) {
    if (!playlist || transitionRef.current || busy) return;
    const targetIndex = playlist.index + delta;
    if (targetIndex < 0 || targetIndex >= playlist.ids.length) return;
    // Never update queue index when the native player rejected a file.
    await loadQueuedItem(playlist.ids, targetIndex, playlist.origin);
  }

  async function playUnqueued(name: "open_local" | "open_https" | "stop",
    args?: Record<string, unknown>) {
    if (transitionRef.current) return;
    transitionRef.current = true;
    consumedEofRef.current = playlist?.sessionId ?? null;
    try {
      const ok = await command(name, args);
      if (ok) {
        setPlaylist(null);
        setEpisodeQueue(null);
        setAutoNextError("");
      }
    } finally {
      transitionRef.current = false;
    }
  }

  useEffect(() => {
    if (transitionRef.current || busy || connectionError) return;
    const nextIndex = nextAtNaturalEof(playlist, snapshot, autoNext, consumedEofRef.current);
    if (nextIndex === null || !playlist) return;
    // Consume this exact EOF synchronously before starting any async command.
    // A persistent Ended snapshot can never advance twice.
    consumedEofRef.current = playlist.sessionId;
    const { ids, origin } = playlist;
    void loadQueuedItem(ids, nextIndex, origin).then(ok => {
      if (!ok) setAutoNextError("下一部自动播放失败，已停止本次自动尝试。请手动选择视频。");
    });
  }, [playlist, snapshot, autoNext, busy, connectionError]);

  const connected = snapshot !== null && !connectionError;
  const active = snapshot?.state === "Playing" || snapshot?.state === "Paused" || snapshot?.state === "Buffering";
  const controllable = connected && active && !busy;
  const error = connectionError || commandError || snapshot?.error?.message;
  const duration = snapshot?.durationSec ?? 0;
  const position = snapshot?.positionSec ?? 0;
  const preview = seekPreview ?? position;
  const seekable = controllable && Number.isFinite(duration) && duration > 0;

  async function applySeek(seconds: number) {
    if (!Number.isFinite(seconds) || seconds < 0) return;
    await command("seek_to", { seconds });
    setSeekPreview(null);
  }

  return <main className={fullscreen ? "fullscreen" : catalogOpen ? "catalog-mode" : ""}>
    <header>
      <h1><i aria-hidden="true" />HongGuoRust</h1>
      <div className="catalog-header-actions">
        <button className="catalog-toggle" disabled={!connected || busy}
          onClick={() => { if (catalogOpen) void backToPlayer(); else void openCatalog(); }}>
          {catalogOpen ? "返回播放" : "内容剧库"}
        </button>
        <span className={connected ? "connection online" : "connection"} role="status">
          {connected ? "已连接" : connectionError ? "连接失败" : "连接中"}
        </span>
      </div>
    </header>
    {catalogOpen
      ? <CatalogView onBack={backToPlayer} onPlay={playFromCatalog} onPlayLocal={playFromLocal} playerError={commandError} />
      : <section aria-label="视频画面" />}
    <footer className="player-controls">
      <div className="player-actions">
        <button disabled={busy || !connected} onClick={() => void playUnqueued("open_local")}>打开本地 MP4</button>
        <input aria-label="公开 HTTPS MP4 直链" type="url" spellCheck={false}
          placeholder="粘贴公开 HTTPS MP4 直链" title={`示例：${SAMPLE}`}
          value={httpsUrl} onChange={e => setHttpsUrl(e.target.value)} />
        <button disabled={busy || !connected || !httpsUrl.trim()}
          onClick={() => void playUnqueued("open_https", { url: httpsUrl.trim() })}>播放 HTTPS</button>
        <button disabled={busy || !connected || snapshot?.state === "Idle"}
          onClick={() => void playUnqueued("stop")}>停止</button>
        <span className="filename" title={snapshot?.filename}>{snapshot?.filename}</span>
        {error && <span role="alert" title={error}>{error}</span>}
      </div>
      <div className="player-timeline">
        <span>{clock(seekPreview ?? snapshot?.positionSec)}</span>
        <input type="range" aria-label="视频进度" min={0} max={Math.max(1, duration)}
          step={0.25} disabled={!seekable} value={Math.min(preview, Math.max(1, duration))}
          onChange={e => setSeekPreview(Number(e.target.value))}
          onPointerUp={() => { if (seekPreview !== null) void applySeek(seekPreview); }}
          onKeyUp={e => { if (["ArrowLeft","ArrowRight","Home","End"].includes(e.key) && seekPreview !== null) void applySeek(seekPreview); }} />
        <span>{clock(snapshot?.durationSec)}</span>
      </div>
      <div className="player-transport">
        <button disabled={!controllable}
          onClick={() => void command("set_paused", { paused: snapshot?.state !== "Paused" })}>
          {snapshot?.state === "Paused" ? "继续" : "暂停"}
        </button>
        {episodeQueue && <>
          <span className="episode-context" title={episodeQueue.seriesTitle}>
            {episodeQueue.seriesTitle} · 第 {episodeQueue.episodes[episodeQueue.index].number} 集
            · {episodeQueue.index + 1}/{episodeQueue.episodes.length}
          </span>
          <button disabled={busy || !connected || !adjacentEpisode(episodeQueue, -1)}
            onClick={() => void moveEpisode(-1)} title="仅在当前作品内切换上一集">上一集</button>
          <button disabled={busy || !connected || !adjacentEpisode(episodeQueue, 1)}
            onClick={() => void moveEpisode(1)} title="仅在当前作品内切换下一集">下一集</button>
          {currentEpisodeEnded(episodeQueue, snapshot) &&
            <span className="episode-completion" role="status">
              {adjacentEpisode(episodeQueue, 1)
                ? "本集已播完，点击“下一集”继续"
                : "本作品已播放到最后一集"}
            </span>}
          {currentEpisodeFailed(episodeQueue, snapshot) &&
            <span className="episode-completion" role="alert">本集播放失败，可重试或切换剧集</span>}
          {currentEpisodeFailed(episodeQueue, snapshot) &&
            <button disabled={busy || !connected}
              onClick={() => void retryEpisode()}>重试本集</button>}
        </>}
        {playlist && <>
          <button disabled={busy || playlist.index === 0}
            onClick={() => void movePlaylist(-1)} title="按本次播放列表顺序播放上一部">上一部</button>
          <span className="playlist-indicator" title={playlist.origin + " · 当前临时播放列表"}>
            列表 {playlist.index + 1}/{playlist.ids.length}
          </span>
          <button disabled={busy || playlist.index >= playlist.ids.length - 1}
            onClick={() => void movePlaylist(1)} title="按本次播放列表顺序播放下一部">下一部</button>
          <label className="playlist-auto-next" title="仅自然播完才自动播放下一部；手动停止或出错不连播">
            <input type="checkbox" checked={autoNext}
              onChange={event => setAutoNext(event.target.checked)} />
            自动连播
          </label>
          {autoNext && snapshot?.state === "Ended" &&
            snapshot.sessionId === playlist.sessionId &&
            playlist.index === playlist.ids.length - 1 &&
            <span className="playlist-indicator" role="status">列表已播放完毕</span>}
          {autoNextError && <span className="playlist-auto-error" role="alert">{autoNextError}</span>}
        </>}
        <label>跳转秒数
          <input aria-label="跳转秒数" type="number" min={0} step={1} value={seekText}
            onChange={e => setSeekText(e.target.value)} />
        </label>
        <button disabled={!controllable || seekText.trim() === "" || !Number.isFinite(Number(seekText)) || Number(seekText) < 0}
          onClick={() => void applySeek(Number(seekText))}>跳转</button>
        <label>倍速
          <select aria-label="播放倍速" value={rate} disabled={!controllable}
            onChange={e => { const next = Number(e.target.value); setRate(next); void command("set_rate", { rate: next }); }}>
            {[0.5, 1, 1.5, 2].map(value => <option value={value} key={value}>{value}×</option>)}
          </select>
        </label>
        <label>音量
          <select aria-label="播放音量" value={volume} disabled={!controllable}
            onChange={e => { const next = Number(e.target.value); setVolume(next); void command("set_volume", { volume: next }); }}>
            {[0, 25, 50, 75, 100].map(value => <option value={value} key={value}>{value}%</option>)}
          </select>
        </label>
        <button disabled={busy || !connected} title="F11 切换全屏，Esc 退出"
          onClick={() => void toggleFullscreen(!fullscreen)}>全屏 F11</button>
        <span className="player-status">{snapshot?.state ?? "—"}</span>
        <span className="player-version">{snapshot?.appVersion ?? "—"}</span>
      </div>
    </footer>
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);
