import { useEffect, useRef, useState, type PointerEvent } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import { formatRuntimeHealth, getRuntimeHealth, type RuntimeHealth } from "./player/runtimeHealth";
import { persistLocalAutoNext, readPlayerPreferences } from "./player/preferences";
import {
  changeMediaTrack, getMediaTracks, selectedTrackId, trackLabel, usableTrackList,
  type TrackKind, type TrackList,
} from "./player/mediaTracks";
import { fetchPreviewFrame, previewBucket, previewTimeAtPointer, validPreviewFrame, type PreviewFrame } from "./player/thumbnailPreview";
import { resolvePlayerShortcut, type PlayerShortcut } from "./player/keyboardShortcuts";
import { volumeAfterMute } from "./player/volume";
import { nextAtNaturalEof, type LocalPlaylist } from "./player/autoNext";
import { CatalogView } from "./catalog/CatalogView";
import type { SeriesDetail } from "./catalog/types";
import {
  adjacentEpisode, createEpisodeQueue, currentEpisodeEnded, currentEpisodeFailed,
  type EpisodeQueue,
} from "./player/episodeQueue";
import "./style.css";

type PlayerCommand = "open_local" | "stop" | "set_paused" | "seek_to"
  | "set_rate" | "set_volume" | "set_fullscreen" | "set_catalog_visible" | "catalog_play_episode" | "library_play";
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
  const [seekText, setSeekText] = useState("0");
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const [previewHover, setPreviewHover] = useState<{ seconds: number; fraction: number } | null>(null);
  const [previewImage, setPreviewImage] = useState<PreviewFrame | null>(null);
  const [previewStatus, setPreviewStatus] = useState<"loading" | "ready" | "unavailable">("loading");
  const previewVersionRef = useRef(0);
  const [rate, setRate] = useState(1);
  const [volume, setVolume] = useState(100);
  const [preferencesReady, setPreferencesReady] = useState(false);
  const [preferencesError, setPreferencesError] = useState("");
  const [healthVisible, setHealthVisible] = useState(false);
  const [healthReport, setHealthReport] = useState<RuntimeHealth | null>(null);
  const [healthBusy, setHealthBusy] = useState(false);
  const [healthError, setHealthError] = useState("");
  const healthRequestRef = useRef(0);
  const [autoNextSaving, setAutoNextSaving] = useState(false);
  const [mediaTracks, setMediaTracks] = useState<TrackList | null>(null);
  const [trackBusy, setTrackBusy] = useState(false);
  const [trackError, setTrackError] = useState("");
  const trackGenerationRef = useRef(0);
  const nativeSessionRef = useRef<number | null>(null);
  const trackFlightRef = useRef(false);
  const [shortcutHelpOpen, setShortcutHelpOpen] = useState(false);
  const lastAudibleVolumeRef = useRef(100);
  const shortcutFlightRef = useRef(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [catalogOpen, setCatalogOpen] = useState(true);
  const catalogVisibilityAppliedRef = useRef(false);
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
    // Rust restores saved native volume/speed for every new libmpv session.
    // Do not reset the UI to 1x/100% on episode changes.
    setSeekPreview(null);
    setSeekText("0");
  }, [snapshot?.sessionId]);

  useEffect(() => {
    let mounted = true;
    void readPlayerPreferences().then(saved => {
      if (!mounted) return;
      setRate(saved.playbackRate);
      setVolume(saved.volume);
      lastAudibleVolumeRef.current = saved.lastAudibleVolume;
      setAutoNext(saved.autoNext);
      setPreferencesError("");
      setPreferencesReady(true);
    }).catch(reason => {
      if (mounted) setPreferencesError("播放器设置无法读取：" + String(reason));
    });
    return () => { mounted = false; };
  }, []);

  async function changeAutoNextSetting(enabled: boolean) {
    if (!preferencesReady || autoNextSaving) return;
    setAutoNextSaving(true);
    setPreferencesError("");
    try {
      const saved = await persistLocalAutoNext(enabled);
      setAutoNext(saved.autoNext);
      lastAudibleVolumeRef.current = saved.lastAudibleVolume;
    } catch (reason) {
      setPreferencesError("自动连播偏好保存失败：" + String(reason));
    } finally {
      setAutoNextSaving(false);
    }
  }

  async function refreshHealth() {
    const request = ++healthRequestRef.current;
    setHealthBusy(true);
    setHealthError("");
    try {
      const report = await getRuntimeHealth();
      if (healthRequestRef.current === request) setHealthReport(report);
    } catch (reason) {
      if (healthRequestRef.current === request) setHealthError(String(reason));
    } finally {
      if (healthRequestRef.current === request) setHealthBusy(false);
    }
  }

  function toggleHealth() {
    if (healthVisible) {
      healthRequestRef.current++;
      setHealthVisible(false);
      setHealthBusy(false);
    } else {
      setHealthVisible(true);
      void refreshHealth();
    }
  }

  async function copyHealth() {
    if (!healthReport) return;
    try {
      await navigator.clipboard.writeText(formatRuntimeHealth(healthReport));
      setHealthError("");
    } catch {
      setHealthError("无法复制诊断信息，请检查系统剪贴板权限");
    }
  }

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

  async function openCatalog() {
    setShortcutHelpOpen(false);
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

  async function playUnqueued(name: "open_local" | "stop",
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
  // Start in the browse surface and hide the native video HWND after IPC is ready.
  useEffect(() => {
    const core = window.__TAURI__?.core;
    if (!core || catalogVisibilityAppliedRef.current) return;
    catalogVisibilityAppliedRef.current = true;
    void core.invoke("set_catalog_visible", { visible: true })
      .catch(reason => setCommandError("首页初始化失败：" + String(reason)));
  }, []);
  const active = snapshot?.state === "Playing" || snapshot?.state === "Paused" || snapshot?.state === "Buffering";
  const controllable = connected && active && !busy;
  const error = connectionError || commandError || snapshot?.error?.message;
  const duration = snapshot?.durationSec ?? 0;
  const position = snapshot?.positionSec ?? 0;
  const preview = seekPreview ?? position;
  const seekable = controllable && Number.isFinite(duration) && duration > 0;
  nativeSessionRef.current = snapshot?.sessionId ?? null;

  useEffect(() => {
    const generation = ++trackGenerationRef.current;
    setMediaTracks(null);
    setTrackError("");
    const id = snapshot?.sessionId;
    if (!connected || catalogOpen || id === null || id === undefined
        || !["Playing", "Paused", "Buffering"].includes(snapshot?.state ?? "")) return;
    void getMediaTracks(id).then(result => {
      if (trackGenerationRef.current !== generation || nativeSessionRef.current !== id) return;
      if (usableTrackList(result, id)) setMediaTracks(result);
    }).catch(() => {
      // Missing track-list must never stop playback or obscure its real error.
      if (trackGenerationRef.current === generation && nativeSessionRef.current === id) {
        setMediaTracks(null);
      }
    });
    return () => { trackGenerationRef.current++; };
  }, [connected, catalogOpen, snapshot?.sessionId, snapshot?.state]);

  async function selectMediaTrack(kind: TrackKind, value: string) {
    const id = snapshot?.sessionId;
    if (id === null || id === undefined || busy || trackBusy || trackFlightRef.current
        || !usableTrackList(mediaTracks, id)) return;
    const trackId = value === "off" && kind === "subtitle" ? null : Number(value);
    if (trackId !== null
        && (!Number.isSafeInteger(trackId) || trackId <= 0
          || !(kind === "audio" ? mediaTracks.audio : mediaTracks.subtitles)
            .some(item => item.id === trackId))) return;
    trackFlightRef.current = true;
    setTrackBusy(true);
    setTrackError("");
    const generation = ++trackGenerationRef.current;
    try {
      const updated = await changeMediaTrack(id, kind, trackId);
      if (nativeSessionRef.current === id && trackGenerationRef.current === generation
          && usableTrackList(updated, id)) {
        setMediaTracks(updated);
      }
    } catch (error) {
      if (nativeSessionRef.current === id && trackGenerationRef.current === generation) {
        setTrackError(String(error));
      }
    } finally {
      trackFlightRef.current = false;
      setTrackBusy(false);
    }
  }
  const previewEnabled = seekable && !catalogOpen && !fullscreen;
  const hoveredBucket = previewHover === null ? null : previewBucket(previewHover.seconds);

  // Debounce by five-second bucket. Old, slow frames are never attached to
  // another hover position or a new native libmpv playback session.
  useEffect(() => {
    const version = ++previewVersionRef.current;
    setPreviewImage(null);
    if (!previewEnabled || hoveredBucket === null || snapshot?.sessionId === null
        || snapshot?.sessionId === undefined) return;
    const sessionId = snapshot.sessionId;
    setPreviewStatus("loading");
    const timer = window.setTimeout(() => {
      void fetchPreviewFrame(sessionId, hoveredBucket).then(frame => {
        if (previewVersionRef.current !== version) return;
        if (validPreviewFrame(frame, sessionId)) {
          setPreviewImage(frame);
          setPreviewStatus("ready");
        } else {
          setPreviewStatus("unavailable");
        }
      }).catch(() => {
        if (previewVersionRef.current === version) setPreviewStatus("unavailable");
      });
    }, 280);
    return () => {
      window.clearTimeout(timer);
      previewVersionRef.current++;
    };
  }, [previewEnabled, hoveredBucket, snapshot?.sessionId]);

  useEffect(() => {
    setPreviewHover(null);
    setPreviewImage(null);
  }, [snapshot?.sessionId, catalogOpen, fullscreen]);

  function updateHover(event: PointerEvent<HTMLInputElement>) {
    if (!previewEnabled) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const seconds = previewTimeAtPointer(event.clientX, bounds.left, bounds.width, duration);
    if (seconds === null) return;
    setPreviewHover({
      seconds,
      fraction: Math.min(1, Math.max(0, seconds / duration)),
    });
  }

  function clearHover() {
    previewVersionRef.current++;
    setPreviewHover(null);
    setPreviewImage(null);
  }

  async function applySeek(seconds: number) {
    if (!Number.isFinite(seconds) || seconds < 0) return;
    await command("seek_to", { seconds });
    setSeekPreview(null);
  }

  async function runShortcut(action: PlayerShortcut) {
    if (action.type === "help") {
      setShortcutHelpOpen(action.value);
      return;
    }
    if (busy || shortcutFlightRef.current) return;
    shortcutFlightRef.current = true;
    try {
      switch (action.type) {
        case "pause":
          await command("set_paused", { paused: snapshot?.state !== "Paused" });
          break;
        case "seek":
          await applySeek(action.seconds);
          break;
        case "volume": {
          const ok = await command("set_volume", { volume: action.value });
          if (ok) {
            if (action.value > 0) lastAudibleVolumeRef.current = action.value;
            setVolume(action.value);
          }
          break;
        }
        case "fullscreen":
          await toggleFullscreen(action.value);
          break;
      }
    } finally {
      shortcutFlightRef.current = false;
    }
  }

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target instanceof Element ? event.target : null;
      const editable = target?.closest("input,textarea,select,button,a,summary,[contenteditable='true'],[role='textbox'],[role='searchbox'],[role='button'],[role='link'],[role='slider']");
      const action = resolvePlayerShortcut({
        key: event.key,
        ctrlKey: event.ctrlKey,
        altKey: event.altKey,
        metaKey: event.metaKey,
        shiftKey: event.shiftKey,
        repeat: event.repeat,
        isComposing: event.isComposing,
        defaultPrevented: event.defaultPrevented,
        targetTag: target?.tagName,
        targetRole: editable?.getAttribute("role"),
        contentEditable: Boolean(editable),
      }, {
        catalogOpen, fullscreen, helpOpen: shortcutHelpOpen,
        connected, controllable, positionSec: snapshot?.positionSec ?? null,
        durationSec: snapshot?.durationSec ?? null,
        volume, lastAudibleVolume: lastAudibleVolumeRef.current,
      });
      if (!action) return;
      event.preventDefault();
      void runShortcut(action);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [catalogOpen, fullscreen, shortcutHelpOpen, connected, controllable,
      snapshot?.positionSec, snapshot?.durationSec, snapshot?.state, volume, busy]);

  async function toggleMute() {
    if (!controllable || !preferencesReady) return;
    const next = volumeAfterMute(volume, lastAudibleVolumeRef.current);
    const ok = await command("set_volume", { volume: next });
    if (!ok) return;
    setVolume(next);
    if (next > 0) lastAudibleVolumeRef.current = next;
  }

  return <main className={fullscreen ? "fullscreen" : catalogOpen ? "catalog-mode" : ""}>
    <header className="app-header">
      <div className="app-brand">
        <h1><i aria-hidden="true" />HongGuoRust</h1>
        <span className="app-brand-label">PERSONAL MEDIA CENTER</span>
      </div>
      <div className="catalog-header-actions">
        <button className="catalog-toggle" disabled={!connected || busy}
          onClick={() => { if (catalogOpen) void backToPlayer(); else void openCatalog(); }}>
          {catalogOpen ? (active ? "返回播放" : "播放器") : "内容首页"}
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
      <div className="player-actions" role="group" aria-label="打开与管理媒体">
        <button className="player-action-secondary" disabled={busy || !connected} onClick={() => void playUnqueued("open_local")}>打开本地 MP4</button>
        <button className="player-action-secondary" disabled={busy || !connected || snapshot?.state === "Idle"}
          onClick={() => void playUnqueued("stop")}>停止</button>
        <span className="filename" title={snapshot?.filename}>{snapshot?.filename}</span>
        {error && <span role="alert" title={error}>{error}</span>}
      </div>
      <div className="player-timeline" role="group" aria-label="播放进度">
        <span>{clock(seekPreview ?? snapshot?.positionSec)}</span>
        <div className="player-range">
          <input type="range" aria-label="视频进度" aria-valuetext={clock(seekPreview ?? snapshot?.positionSec) + " / " + clock(snapshot?.durationSec)} min={0} max={Math.max(1, duration)}
            step={0.25} disabled={!seekable} value={Math.min(preview, Math.max(1, duration))}
            onPointerEnter={updateHover} onPointerMove={updateHover}
            onPointerLeave={clearHover} onBlur={clearHover}
            onChange={e => setSeekPreview(Number(e.target.value))}
            onPointerUp={() => { if (seekPreview !== null) void applySeek(seekPreview); }}
            onKeyUp={e => { if (["ArrowLeft","ArrowRight","Home","End"].includes(e.key) && seekPreview !== null) void applySeek(seekPreview); }} />
          {previewEnabled && previewHover !== null && <div className="player-preview-popover"
            role="status" aria-live="off"
            style={{ left: Math.min(92, Math.max(8, previewHover.fraction * 100)) + "%" }}>
            {previewImage && previewStatus === "ready"
              ? <img src={previewImage.dataUrl} alt="" decoding="async" />
              : <span className="player-preview-placeholder">
                  {previewStatus === "loading" ? "正在生成预览…" : "该位置暂无缩略图"}
                </span>}
            <strong>{clock(previewHover.seconds)}</strong>
          </div>}
        </div>
        <span>{clock(snapshot?.durationSec)}</span>
      </div>
      <div className="player-transport" role="group" aria-label="播放器控制栏">
        <div className="player-transport-group player-transport-primary" role="group" aria-label="播放与剧集导航">
          <button className="player-control-primary" disabled={!controllable}
            aria-label={snapshot?.state === "Paused" ? "继续播放" : "暂停播放"}
            aria-keyshortcuts="Space K" title="空格 / K：暂停或继续"
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
                disabled={!preferencesReady || autoNextSaving}
                onChange={event => { void changeAutoNextSetting(event.target.checked); }} />
              自动连播
            </label>
            {autoNext && snapshot?.state === "Ended" &&
              snapshot.sessionId === playlist.sessionId &&
              playlist.index === playlist.ids.length - 1 &&
              <span className="playlist-indicator" role="status">列表已播放完毕</span>}
            {autoNextError && <span className="playlist-auto-error" role="alert">{autoNextError}</span>}
          </>}
        </div>
        <span className="player-transport-separator" aria-hidden="true" />
        <div className="player-transport-group player-transport-seek" role="group" aria-label="跳转控制">
          <label>跳转秒数
            <input aria-label="跳转秒数" type="number" min={0} step={1} value={seekText}
              onChange={e => setSeekText(e.target.value)} />
          </label>
          <button disabled={!controllable || seekText.trim() === "" || !Number.isFinite(Number(seekText)) || Number(seekText) < 0}
            onClick={() => void applySeek(Number(seekText))}>跳转</button>
        </div>
        <span className="player-transport-separator" aria-hidden="true" />
        <div className="player-transport-group player-transport-media" role="group" aria-label="音轨与播放设置">
          <label>音轨
            <select aria-label="音轨" disabled={!controllable || busy || trackBusy || !mediaTracks || mediaTracks.audio.length === 0}
              value={mediaTracks ? selectedTrackId(mediaTracks.audio, false) : ""}
              onChange={event => { void selectMediaTrack("audio", event.target.value); }}>
              <option value="" disabled>无可用音轨</option>
              {mediaTracks?.audio.map((track, index) =>
                <option value={String(track.id)} key={track.id}>{trackLabel(track, index)}</option>)}
            </select>
          </label>
          <label>字幕
            <select aria-label="字幕轨道" disabled={!controllable || busy || trackBusy || !mediaTracks || mediaTracks.subtitles.length === 0}
              value={mediaTracks ? selectedTrackId(mediaTracks.subtitles, true) : "off"}
              onChange={event => { void selectMediaTrack("subtitle", event.target.value); }}>
              <option value="off">关闭字幕</option>
              {mediaTracks?.subtitles.map((track, index) =>
                <option value={String(track.id)} key={track.id}>{trackLabel(track, index)}</option>)}
            </select>
          </label>
          {trackError && <span className="player-track-error" role="alert">{trackError}</span>}
          <label>倍速
            <select aria-label="播放倍速" value={rate} disabled={!controllable || !preferencesReady}
              title="保存为下次播放的默认倍速"
              onChange={e => {
                const next = Number(e.target.value);
                void command("set_rate", { rate: next }).then(ok => {
                  if (ok) setRate(next);
                });
              }}>
              {[0.5, 1, 1.5, 2].map(value => <option value={value} key={value}>{value}×</option>)}
              {![0.5, 1, 1.5, 2].includes(rate) && <option value={rate}>{rate}×</option>}
            </select>
          </label>
          <label>音量
            <select aria-label="播放音量" value={volume} disabled={!controllable || !preferencesReady}
              title="音量与静音状态自动保存"
              onChange={e => {
                const next = Number(e.target.value);
                void command("set_volume", { volume: next }).then(ok => {
                  if (!ok) return;
                  setVolume(next);
                  if (next > 0) lastAudibleVolumeRef.current = next;
                });
              }}>
              {[0, 25, 50, 75, 100].map(value => <option value={value} key={value}>{value}%</option>)}
              {![0, 25, 50, 75, 100].includes(volume) && <option value={volume}>{volume}%</option>}
            </select>
          </label>
          <button type="button" className="player-mute-toggle" aria-pressed={volume === 0}
            aria-label={volume === 0 ? "取消静音并恢复上次音量" : "静音"}
            title={volume === 0 ? "恢复上次非静音音量" : "静音"}
            disabled={!controllable || !preferencesReady} onClick={() => void toggleMute()}>
            {volume === 0 ? "恢复声音" : "静音"}
          </button>
        </div>
        <span className="player-transport-separator" aria-hidden="true" />
        <div className="player-transport-group player-transport-utility" role="group" aria-label="播放器工具">
          <button disabled={busy || !connected} aria-keyshortcuts="F11 F"
            title="F 或 F11 切换全屏，Esc 退出"
            onClick={() => void toggleFullscreen(!fullscreen)}>全屏 F11</button>
          <button type="button" className="player-help-trigger"
            aria-expanded={shortcutHelpOpen} aria-controls="player-shortcut-help"
            aria-keyshortcuts="Shift+Slash"
            onClick={() => setShortcutHelpOpen(value => !value)}>快捷键 ?</button>
          {preferencesError && <span role="alert" className="player-preferences-error"
            title={preferencesError}>设置异常</span>}
          <span className="player-preferences-hint" title="倍速、音量与本地列表自动连播偏好保存在本机">
            {preferencesReady ? "设置自动保存" : "设置未加载"}
          </span>
          {import.meta.env.DEV && <>
            <button type="button" aria-expanded={healthVisible}
              title="只读诊断：仅含当前进程的会话、缓冲、退出统计，不采集视频路径和 URL"
              onClick={toggleHealth}>运行诊断</button>
            {healthVisible && <>
              <button type="button" disabled={healthBusy || !connected} onClick={() => void refreshHealth()}>
                {healthBusy ? "读取中…" : "刷新"}
              </button>
              {healthReport && <span className="player-health-summary" role="status"
                title={formatRuntimeHealth(healthReport)}>
                打开 {healthReport.sessionsOpened} · 缓冲 {healthReport.bufferEvents}
                /{healthReport.bufferTotalMs}ms · 释放 {healthReport.libmpvReleases}
                /{healthReport.maxReleaseMs}ms
              </span>}
              <button type="button" disabled={!healthReport} onClick={() => void copyHealth()}>
                复制诊断
              </button>
              {healthError && <span role="alert" className="player-health-error"
                title={healthError}>诊断读取/复制失败</span>}
            </>}
            <span className="player-status">{snapshot?.state ?? "—"}</span>
            <span className="player-version">{snapshot?.appVersion ?? "—"}</span>
          </>}
        </div>
      </div>
    </footer>
    {!catalogOpen && shortcutHelpOpen && <aside id="player-shortcut-help"
      className="player-shortcut-help" role="dialog" aria-label="播放器快捷键说明" aria-modal="false">
      <div className="shortcut-help-heading">
        <strong>键盘快捷键</strong>
        <button type="button" onClick={() => setShortcutHelpOpen(false)}>关闭 Esc</button>
      </div>
      <dl>
        <div><dt>空格 / K</dt><dd>暂停或继续</dd></div>
        <div><dt>← / →</dt><dd>后退 / 前进 5 秒</dd></div>
        <div><dt>J / L</dt><dd>后退 / 前进 10 秒</dd></div>
        <div><dt>↑ / ↓</dt><dd>音量 ±25%</dd></div>
        <div><dt>M</dt><dd>静音 / 恢复音量</dd></div>
        <div><dt>F / F11</dt><dd>全屏切换</dd></div>
        <div><dt>Esc</dt><dd>关闭提示 / 退出全屏</dd></div>
        <div><dt>?</dt><dd>显示 / 隐藏快捷键</dd></div>
      </dl>
      <p>输入框、搜索框和选集操作时不触发播放快捷键；Ctrl/Alt/系统组合键保留原有行为。</p>
    </aside>}
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);
