import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import { persistLocalAutoNext, readPlayerPreferences } from "./player/preferences";
import {
  changeMediaTrack, getMediaTracks, selectedTrackId, trackLabel, usableTrackList,
  type TrackKind, type TrackList,
} from "./player/mediaTracks";
import {
  fetchPreviewFrame, previewBucket, previewTimeAtPointer, validPreviewFrame, type PreviewFrame,
} from "./player/thumbnailPreview";
import { resolvePlayerShortcut, type PlayerShortcut } from "./player/keyboardShortcuts";
import { volumeAfterMute } from "./player/volume";
import { nextAtNaturalEof, type LocalPlaylist } from "./player/autoNext";
import {
  adjacentEpisode, currentEpisodeEnded, currentEpisodeFailed, type EpisodeQueue,
} from "./player/episodeQueue";
import {
  dismissPlayerOverlay, fetchPlayerOverlayContext, isOverlayContext,
  PLAYER_CONTEXT_EVENT, updatePlayerOverlayContext, type PlayerOverlayContext,
} from "./player/overlayContext";
import "./overlay.css";

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

function formatClock(value?: number): string {
  if (value === undefined || !Number.isFinite(value)) return "00:00";
  const whole = Math.floor(Math.max(0, value));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const seconds = whole % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

type PopoverKind = "none" | "speed" | "tracks" | "shortcuts";
type PulseKind = "play" | "pause" | "back" | "forward" | "mute" | "volume";

interface PulseState {
  id: number;
  kind: PulseKind;
  label?: string;
}

function OverlayApp() {
  const [context, setContext] = useState<PlayerOverlayContext | null>(null);
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [commandError, setCommandError] = useState("");
  const [busy, setBusy] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);

  // Preferences
  const [rate, setRate] = useState(1);
  const [volume, setVolume] = useState(100);
  const [volumeDraft, setVolumeDraft] = useState<number | null>(null);
  const [autoNext, setAutoNext] = useState(false);
  const [preferencesReady, setPreferencesReady] = useState(false);
  const lastAudibleVolumeRef = useRef(100);

  // Seek & thumbnail preview
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const [previewHover, setPreviewHover] = useState<{ seconds: number; fraction: number } | null>(null);
  const [previewImage, setPreviewImage] = useState<PreviewFrame | null>(null);
  const [previewStatus, setPreviewStatus] = useState<"loading" | "ready" | "unavailable">("loading");
  const previewVersionRef = useRef(0);

  // Media tracks
  const [mediaTracks, setMediaTracks] = useState<TrackList | null>(null);
  const [trackBusy, setTrackBusy] = useState(false);
  const [trackError, setTrackError] = useState("");
  const trackGenerationRef = useRef(0);
  const trackFlightRef = useRef(false);
  const nativeSessionRef = useRef<number | null>(null);

  // Chrome & panels
  const [hudVisible, setHudVisible] = useState(true);
  const [hoverChrome, setHoverChrome] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [popover, setPopover] = useState<PopoverKind>("none");
  const [pulse, setPulse] = useState<PulseState | null>(null);
  const hideTimerRef = useRef<number | null>(null);
  const clickTimerRef = useRef<number | null>(null);
  const pulseCounterRef = useRef(0);
  const pulseTimerRef = useRef<number | null>(null);
  const transitionRef = useRef(false);
  const shortcutFlightRef = useRef(false);
  const consumedEofRef = useRef<number | null>(null);

  const showPulse = useCallback((kind: PulseKind, label?: string) => {
    const id = ++pulseCounterRef.current;
    setPulse({ id, kind, label });
    if (pulseTimerRef.current !== null) window.clearTimeout(pulseTimerRef.current);
    pulseTimerRef.current = window.setTimeout(() => {
      setPulse(current => (current?.id === id ? null : current));
    }, 650);
  }, []);

  const revealHud = useCallback(() => {
    setHudVisible(true);
    if (hideTimerRef.current !== null) window.clearTimeout(hideTimerRef.current);
    hideTimerRef.current = window.setTimeout(() => {
      setHudVisible(false);
    }, 2800);
  }, []);

  // Load initial context & subscribe to context updates from main window / Rust.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | null = null;

    void fetchPlayerOverlayContext().then(initial => {
      if (alive && initial) {
        setContext(initial);
        setDrawerOpen(false);
        setPopover("none");
        setCommandError("");
        revealHud();
      }
    }).catch(() => {});

    const listen = window.__TAURI__?.event?.listen;
    if (listen) {
      void listen<unknown>(PLAYER_CONTEXT_EVENT, event => {
        if (!alive) return;
        if (isOverlayContext(event.payload)) {
          setContext(event.payload);
          setCommandError("");
          setPopover("none");
          revealHud();
        } else {
          setContext(null);
          setDrawerOpen(false);
          setPopover("none");
        }
      }).then(fn => {
        if (alive) unlisten = fn;
        else fn();
      }).catch(() => {});
    }

    return () => {
      alive = false;
      unlisten?.();
    };
  }, [revealHud]);

  // Poll native player snapshot while active.
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function refresh() {
      let delay = context ? 200 : 800;
      try {
        const result = await getSnapshot();
        if (!active) return;
        setSnapshot(result);
      } catch {
        delay = 800;
      }
      timer = setTimeout(refresh, delay);
    }
    void refresh();
    return () => { active = false; clearTimeout(timer); };
  }, [context]);

  // Poll fullscreen state.
  useEffect(() => {
    let mounted = true;
    const syncFullscreen = () => {
      void window.__TAURI__?.core.invoke<boolean>("get_fullscreen").then(value => {
        if (mounted) setFullscreen(value);
      }).catch(() => {});
    };
    syncFullscreen();
    const timer = setInterval(syncFullscreen, 700);
    return () => { mounted = false; clearInterval(timer); };
  }, []);

  // Load persisted player preferences.
  useEffect(() => {
    let mounted = true;
    void readPlayerPreferences().then(saved => {
      if (!mounted) return;
      setRate(saved.playbackRate);
      setVolume(saved.volume);
      lastAudibleVolumeRef.current = saved.lastAudibleVolume;
      setAutoNext(saved.autoNext);
      setPreferencesReady(true);
    }).catch(() => {});
    return () => { mounted = false; };
  }, [context?.sessionId]);

  useEffect(() => {
    setSeekPreview(null);
    setPreviewHover(null);
    setPreviewImage(null);
  }, [snapshot?.sessionId]);

  // Cleanup timers on unmount.
  useEffect(() => () => {
    if (hideTimerRef.current !== null) window.clearTimeout(hideTimerRef.current);
    if (clickTimerRef.current !== null) window.clearTimeout(clickTimerRef.current);
    if (pulseTimerRef.current !== null) window.clearTimeout(pulseTimerRef.current);
  }, []);

  const invokePlayer = useCallback(async (cmd: string, args?: Record<string, unknown>): Promise<boolean> => {
    setBusy(true);
    setCommandError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("播放器桥接不可用");
      await core.invoke(cmd, args);
      return true;
    } catch (reason) {
      setCommandError(String(reason));
      return false;
    } finally {
      setBusy(false);
    }
  }, []);

  const active = snapshot?.state === "Playing" || snapshot?.state === "Paused" || snapshot?.state === "Buffering";
  const connected = snapshot !== null;
  const controllable = connected && active && !busy;
  const duration = snapshot?.durationSec ?? 0;
  const position = snapshot?.positionSec ?? 0;
  const displayPosition = seekPreview ?? position;
  const seekable = controllable && Number.isFinite(duration) && duration > 0;
  const progressRatio = duration > 0 ? Math.min(100, Math.max(0, (displayPosition / duration) * 100)) : 0;
  nativeSessionRef.current = snapshot?.sessionId ?? null;

  // Derived episode queue / local playlist
  const episodeQueue: EpisodeQueue | null = context?.kind === "series" ? {
    seriesId: context.seriesId,
    seriesTitle: context.seriesTitle,
    episodes: context.episodes,
    index: context.index,
    sessionId: context.sessionId,
  } : null;

  const localPlaylist: LocalPlaylist | null = context?.kind === "local" ? {
    ids: context.ids,
    index: context.index,
    origin: context.origin,
    sessionId: context.sessionId,
  } : null;

  // Load media tracks for active session.
  useEffect(() => {
    const generation = ++trackGenerationRef.current;
    setMediaTracks(null);
    setTrackError("");
    const id = snapshot?.sessionId;
    if (!connected || !context || id === null || id === undefined
        || !["Playing", "Paused", "Buffering"].includes(snapshot?.state ?? "")) return;
    void getMediaTracks(id).then(result => {
      if (trackGenerationRef.current !== generation || nativeSessionRef.current !== id) return;
      if (usableTrackList(result, id)) setMediaTracks(result);
    }).catch(() => {
      if (trackGenerationRef.current === generation && nativeSessionRef.current === id) {
        setMediaTracks(null);
      }
    });
    return () => { trackGenerationRef.current++; };
  }, [connected, context, snapshot?.sessionId, snapshot?.state]);

  // Thumbnail hover preview fetch.
  const previewEnabled = seekable;
  const hoveredBucket = previewHover === null ? null : previewBucket(previewHover.seconds);
  useEffect(() => {
    const version = ++previewVersionRef.current;
    setPreviewImage(null);
    if (!previewEnabled || hoveredBucket === null || snapshot?.sessionId == null) return;
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
    }, 240);
    return () => {
      window.clearTimeout(timer);
      previewVersionRef.current++;
    };
  }, [previewEnabled, hoveredBucket, snapshot?.sessionId]);

  const togglePlayPause = useCallback(async () => {
    if (!controllable) return;
    const willPause = snapshot?.state !== "Paused";
    const ok = await invokePlayer("set_paused", { paused: willPause });
    if (ok) {
      showPulse(willPause ? "pause" : "play");
      revealHud();
    }
  }, [controllable, snapshot?.state, invokePlayer, showPulse, revealHud]);

  const applySeek = useCallback(async (seconds: number, feedback?: "back" | "forward") => {
    if (!Number.isFinite(seconds)) return;
    const clamped = Math.max(0, duration > 0 ? Math.min(Math.max(0, duration - 0.25), seconds) : seconds);
    const ok = await invokePlayer("seek_to", { seconds: clamped });
    setSeekPreview(null);
    if (ok && feedback) showPulse(feedback, feedback === "back" ? "-10s" : "+10s");
  }, [duration, invokePlayer, showPulse]);

  const commitVolume = useCallback(async (next: number, showFeedback = false) => {
    const clamped = Math.round(Math.min(100, Math.max(0, next)));
    const ok = await invokePlayer("set_volume", { volume: clamped });
    setVolumeDraft(null);
    if (ok) {
      setVolume(clamped);
      if (clamped > 0) lastAudibleVolumeRef.current = clamped;
      if (showFeedback) showPulse(clamped === 0 ? "mute" : "volume", `${clamped}%`);
    }
  }, [invokePlayer, showPulse]);

  const toggleMute = useCallback(async () => {
    if (!controllable || !preferencesReady) return;
    const next = volumeAfterMute(volume, lastAudibleVolumeRef.current);
    await commitVolume(next, true);
  }, [controllable, preferencesReady, volume, commitVolume]);

  const toggleFullscreen = useCallback(async (target: boolean) => {
    const ok = await invokePlayer("set_fullscreen", { fullscreen: target });
    if (ok) setFullscreen(target);
  }, [invokePlayer]);

  const handleDismiss = useCallback(async () => {
    if (transitionRef.current) return;
    transitionRef.current = true;
    try {
      setDrawerOpen(false);
      setPopover("none");
      await dismissPlayerOverlay();
      setContext(null);
    } catch (reason) {
      setCommandError(String(reason));
    } finally {
      transitionRef.current = false;
    }
  }, []);

  const openSeriesEpisode = useCallback(async (targetIndex: number) => {
    if (context?.kind !== "series" || transitionRef.current || busy) return;
    const episode = context.episodes[targetIndex];
    if (!episode || episode.playable === false) return;
    transitionRef.current = true;
    setBusy(true);
    setCommandError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("桌面桥接不可用");
      const sessionId = await core.invoke<number>("catalog_play_episode", {
        seriesId: context.seriesId,
        episodeId: episode.id,
      });
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("选集播放失败");
      }
      const nextContext: PlayerOverlayContext = {
        ...context,
        index: targetIndex,
        sessionId,
      };
      setContext(nextContext);
      setDrawerOpen(false);
      await updatePlayerOverlayContext(nextContext);
      revealHud();
    } catch (reason) {
      setCommandError(String(reason));
    } finally {
      transitionRef.current = false;
      setBusy(false);
    }
  }, [context, busy, revealHud]);

  const openLocalIndex = useCallback(async (targetIndex: number) => {
    if (context?.kind !== "local" || transitionRef.current || busy) return;
    if (targetIndex < 0 || targetIndex >= context.ids.length) return;
    transitionRef.current = true;
    setBusy(true);
    setCommandError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("桌面桥接不可用");
      const sessionId = await core.invoke<number>("library_play", { id: context.ids[targetIndex] });
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("视频打开失败");
      }
      const nextContext: PlayerOverlayContext = {
        ...context,
        index: targetIndex,
        sessionId,
      };
      setContext(nextContext);
      await updatePlayerOverlayContext(nextContext);
      revealHud();
    } catch (reason) {
      setCommandError(String(reason));
    } finally {
      transitionRef.current = false;
      setBusy(false);
    }
  }, [context, busy, revealHud]);

  // Local playlist auto-next on natural EOF.
  useEffect(() => {
    if (transitionRef.current || busy) return;
    const nextIndex = nextAtNaturalEof(localPlaylist, snapshot, autoNext, consumedEofRef.current);
    if (nextIndex === null || !localPlaylist) return;
    consumedEofRef.current = localPlaylist.sessionId;
    void openLocalIndex(nextIndex);
  }, [localPlaylist, snapshot, autoNext, busy, openLocalIndex]);

  const selectMediaTrack = useCallback(async (kind: TrackKind, value: string) => {
    const id = snapshot?.sessionId;
    if (id == null || busy || trackBusy || trackFlightRef.current || !usableTrackList(mediaTracks, id)) return;
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
    } catch (reason) {
      if (nativeSessionRef.current === id && trackGenerationRef.current === generation) {
        setTrackError(String(reason));
      }
    } finally {
      trackFlightRef.current = false;
      setTrackBusy(false);
    }
  }, [snapshot?.sessionId, busy, trackBusy, mediaTracks]);

  const runShortcut = useCallback(async (action: PlayerShortcut) => {
    if (action.type === "help") {
      setPopover(action.value ? "shortcuts" : "none");
      revealHud();
      return;
    }
    if (busy || shortcutFlightRef.current) return;
    shortcutFlightRef.current = true;
    try {
      switch (action.type) {
        case "pause":
          await togglePlayPause();
          break;
        case "seek": {
          const delta = action.seconds - position;
          await applySeek(action.seconds, delta < 0 ? "back" : "forward");
          break;
        }
        case "volume":
          await commitVolume(action.value, true);
          break;
        case "fullscreen":
          await toggleFullscreen(action.value);
          break;
      }
    } finally {
      shortcutFlightRef.current = false;
    }
  }, [busy, togglePlayPause, position, applySeek, commitVolume, toggleFullscreen, revealHud]);

  // Keyboard shortcuts on overlay window.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      revealHud();
      if (event.key === "Escape") {
        if (popover !== "none") {
          event.preventDefault();
          setPopover("none");
          return;
        }
        if (drawerOpen) {
          event.preventDefault();
          setDrawerOpen(false);
          return;
        }
        if (!fullscreen) {
          event.preventDefault();
          void handleDismiss();
          return;
        }
      }
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
        catalogOpen: false,
        fullscreen,
        helpOpen: popover === "shortcuts",
        connected,
        controllable,
        positionSec: snapshot?.positionSec ?? null,
        durationSec: snapshot?.durationSec ?? null,
        volume,
        lastAudibleVolume: lastAudibleVolumeRef.current,
      });
      if (!action) return;
      event.preventDefault();
      void runShortcut(action);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [popover, drawerOpen, fullscreen, connected, controllable, snapshot?.positionSec,
      snapshot?.durationSec, volume, runShortcut, handleDismiss, revealHud]);

  function onStageClick() {
    if (popover !== "none") {
      setPopover("none");
      return;
    }
    if (drawerOpen) {
      setDrawerOpen(false);
      return;
    }
    if (clickTimerRef.current !== null) window.clearTimeout(clickTimerRef.current);
    clickTimerRef.current = window.setTimeout(() => {
      clickTimerRef.current = null;
      void togglePlayPause();
    }, 210);
  }

  function onStageDoubleClick() {
    if (clickTimerRef.current !== null) {
      window.clearTimeout(clickTimerRef.current);
      clickTimerRef.current = null;
    }
    void toggleFullscreen(!fullscreen);
  }

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

  const isLoading = snapshot?.state === "Loading" || snapshot?.state === "Buffering";
  const isPaused = snapshot?.state === "Paused";
  const isEnded = snapshot?.state === "Ended";
  const playbackError = commandError || snapshot?.error?.message || "";
  const seriesEnded = currentEpisodeEnded(episodeQueue, snapshot);
  const seriesFailed = currentEpisodeFailed(episodeQueue, snapshot);
  const prevEp = adjacentEpisode(episodeQueue, -1);
  const nextEp = adjacentEpisode(episodeQueue, 1);

  const pinHud = isPaused || isLoading || isEnded || Boolean(playbackError)
    || drawerOpen || popover !== "none" || hoverChrome || seekPreview !== null;
  const chromeVisible = hudVisible || pinHud;

  const headingTitle = context?.kind === "series"
    ? context.seriesTitle
    : context?.kind === "local"
      ? (context.titles[String(context.ids[context.index])] || snapshot?.filename || "本地视频")
      : ((context?.title && context.title !== "本地视频" ? context.title : snapshot?.filename) || "本地视频");

  const headingSubtitle = context?.kind === "series"
    ? `第 ${context.episodes[context.index].number} 集 · ${context.episodes[context.index].title}`
    : context?.kind === "local" && context.ids.length > 1
      ? `${context.origin} · ${context.index + 1} / ${context.ids.length}`
      : "";

  const displayVolume = volumeDraft ?? volume;
  const hasTracks = Boolean(mediaTracks && (mediaTracks.audio.length > 1 || mediaTracks.subtitles.length > 0));

  return (
    <div
      className={"nf-overlay" + (chromeVisible ? " is-chrome-visible" : " is-chrome-hidden")}
      onPointerMove={revealHud}
      onPointerDown={revealHud}
    >
      {/* Clickable transparent video surface */}
      <div
        className="nf-stage"
        onClick={onStageClick}
        onDoubleClick={onStageDoubleClick}
      />

      {/* Center transient HUD pulse */}
      {pulse && (
        <div key={pulse.id} className="nf-pulse" aria-hidden="true">
          {pulse.kind === "play" && (
            <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
          )}
          {pulse.kind === "pause" && (
            <svg viewBox="0 0 24 24"><path d="M6 5h4v14H6zm8 0h4v14h-4z" fill="currentColor" /></svg>
          )}
          {(pulse.kind === "back" || pulse.kind === "forward" || pulse.kind === "mute" || pulse.kind === "volume") && (
            <span>{pulse.label}</span>
          )}
        </div>
      )}

      {/* Center Netflix red buffering spinner */}
      {isLoading && !playbackError && (
        <div className="nf-spinner-wrap" role="status" aria-label="正在缓冲">
          <span className="nf-spinner" />
        </div>
      )}

      {/* Center error / end-of-episode card */}
      {(playbackError || seriesFailed) && (
        <div className="nf-center-card" role="alert">
          <h3>播放遇到问题</h3>
          <p>{playbackError || "当前剧集未能正常播放，你可以重试或切换其他剧集。"}</p>
          <div className="nf-center-actions">
            {context?.kind === "series" && (
              <button type="button" className="nf-btn-white" disabled={busy}
                onClick={() => void openSeriesEpisode(context.index)}>
                重试本集
              </button>
            )}
            {context?.kind === "local" && (
              <button type="button" className="nf-btn-white" disabled={busy}
                onClick={() => void openLocalIndex(context.index)}>
                重新打开
              </button>
            )}
            <button type="button" className="nf-btn-glass" onClick={() => void handleDismiss()}>
              返回片库
            </button>
          </div>
        </div>
      )}

      {!playbackError && !seriesFailed && (seriesEnded || (isEnded && context?.kind !== "local")) && (
        <div className="nf-center-card" role="status">
          <span className="nf-center-kicker">{context?.kind === "series" && nextEp ? "即将观看" : "播放结束"}</span>
          <h3>
            {context?.kind === "series" && nextEp
              ? `第 ${nextEp.episode.number} 集 · ${nextEp.episode.title}`
              : "本集已观看完毕"}
          </h3>
          <div className="nf-center-actions">
            {context?.kind === "series" && nextEp && nextEp.episode.playable !== false && (
              <button type="button" className="nf-btn-white" disabled={busy}
                onClick={() => void openSeriesEpisode(nextEp.index)}>
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                播放下一集
              </button>
            )}
            {context?.kind === "series" && (
              <button type="button" className="nf-btn-glass" disabled={busy}
                onClick={() => void openSeriesEpisode(context.index)}>
                重播本集
              </button>
            )}
            <button type="button" className="nf-btn-glass" onClick={() => void handleDismiss()}>
              返回片库
            </button>
          </div>
        </div>
      )}

      {/* Top Gradient Header */}
      <header
        className="nf-top-bar"
        onPointerEnter={() => setHoverChrome(true)}
        onPointerLeave={() => setHoverChrome(false)}
      >
        <div className="nf-top-left">
          <button
            type="button"
            className="nf-icon-btn nf-back-btn"
            aria-label="返回片库"
            title="返回片库 (Esc)"
            onClick={() => void handleDismiss()}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M19 12H5m0 0 7 7m-7-7 7-7" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
          <div className="nf-title-stack">
            <strong title={headingTitle}>{headingTitle}</strong>
            {headingSubtitle && <span title={headingSubtitle}>{headingSubtitle}</span>}
          </div>
        </div>

        <div className="nf-top-right">
          {context?.kind === "series" && (
            <button
              type="button"
              className={"nf-pill-btn" + (drawerOpen ? " is-active" : "")}
              onClick={() => { setPopover("none"); setDrawerOpen(v => !v); }}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M4 6h16M4 12h16M4 18h10" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
              </svg>
              选集 ({context.index + 1}/{context.episodes.length})
            </button>
          )}
          <button
            type="button"
            className={"nf-icon-btn" + (popover === "shortcuts" ? " is-active" : "")}
            aria-label="快捷键说明"
            title="快捷键 (?)"
            onClick={() => setPopover(p => (p === "shortcuts" ? "none" : "shortcuts"))}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="2.5" y="5.5" width="19" height="13" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.8" />
              <path d="M6.5 9.5h1.5M10.5 9.5h1.5M14.5 9.5h1.5M7 14.5h10" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
            </svg>
          </button>
        </div>
      </header>

      {/* Bottom Gradient Controls */}
      <footer
        className="nf-bottom-bar"
        onPointerEnter={() => setHoverChrome(true)}
        onPointerLeave={() => setHoverChrome(false)}
      >
        {/* Scrubbing Timeline */}
        <div className="nf-timeline-row">
          <div className="nf-scrub-wrap">
            {previewEnabled && previewHover !== null && (
              <div
                className="nf-preview-popover"
                style={{ left: `${Math.min(91, Math.max(9, previewHover.fraction * 100))}%` }}
              >
                <div className="nf-preview-frame">
                  {previewImage && previewStatus === "ready" ? (
                    <img src={previewImage.dataUrl} alt="" decoding="async" />
                  ) : (
                    <span>{previewStatus === "loading" ? "预览生成中…" : formatClock(previewHover.seconds)}</span>
                  )}
                </div>
                <strong>{formatClock(previewHover.seconds)}</strong>
              </div>
            )}
            <div className="nf-scrub-track" aria-hidden="true">
              <div className="nf-scrub-fill" style={{ width: `${progressRatio}%` }} />
            </div>
            <input
              type="range"
              className="nf-scrub-input"
              aria-label="播放进度"
              min={0}
              max={Math.max(1, duration)}
              step={0.25}
              disabled={!seekable}
              value={Math.min(displayPosition, Math.max(1, duration))}
              onPointerEnter={updateHover}
              onPointerMove={updateHover}
              onPointerLeave={clearHover}
              onBlur={clearHover}
              onChange={e => setSeekPreview(Number(e.target.value))}
              onPointerUp={() => { if (seekPreview !== null) void applySeek(seekPreview); }}
              onKeyUp={e => {
                if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key) && seekPreview !== null) {
                  void applySeek(seekPreview);
                }
              }}
            />
          </div>
          <span className="nf-time-readout">
            {formatClock(displayPosition)} <i>/</i> {formatClock(snapshot?.durationSec)}
          </span>
        </div>

        {/* Transport Controls Row */}
        <div className="nf-controls-row">
          <div className="nf-controls-left">
            <button
              type="button"
              className="nf-icon-btn nf-play-btn"
              disabled={!controllable}
              aria-label={isPaused ? "继续播放" : "暂停播放"}
              title={isPaused ? "继续 (Space)" : "暂停 (Space)"}
              onClick={() => void togglePlayPause()}
            >
              {isPaused ? (
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12.5-7.5z" fill="currentColor" /></svg>
              ) : (
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 4.5h4.2v15H6zm7.8 0H18v15h-4.2z" fill="currentColor" /></svg>
              )}
            </button>

            <button
              type="button"
              className="nf-icon-btn"
              disabled={!seekable}
              aria-label="后退 10 秒"
              title="后退 10 秒 (J)"
              onClick={() => void applySeek(position - 10, "back")}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 5a7 7 0 1 1-6.7 9" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                <path d="M8.5 5H12V1.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                <text x="12" y="14.5" textAnchor="middle" fill="currentColor" fontSize="6.8" fontWeight="700">10</text>
              </svg>
            </button>

            <button
              type="button"
              className="nf-icon-btn"
              disabled={!seekable}
              aria-label="前进 10 秒"
              title="前进 10 秒 (L)"
              onClick={() => void applySeek(position + 10, "forward")}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 5a7 7 0 1 0 6.7 9" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                <path d="M15.5 5H12V1.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                <text x="12" y="14.5" textAnchor="middle" fill="currentColor" fontSize="6.8" fontWeight="700">10</text>
              </svg>
            </button>

            {/* Series prev/next */}
            {context?.kind === "series" && (
              <>
                <button
                  type="button"
                  className="nf-icon-btn"
                  disabled={busy || !prevEp || prevEp.episode.playable === false}
                  aria-label="上一集"
                  title="上一集"
                  onClick={() => { if (prevEp) void openSeriesEpisode(prevEp.index); }}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M6 5v14M18 5l-9 7 9 7V5z" fill="currentColor" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                  </svg>
                </button>
                <button
                  type="button"
                  className="nf-icon-btn"
                  disabled={busy || !nextEp || nextEp.episode.playable === false}
                  aria-label="下一集"
                  title={nextEp ? `下一集：第 ${nextEp.episode.number} 集` : "已是最后一集"}
                  onClick={() => { if (nextEp) void openSeriesEpisode(nextEp.index); }}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M18 5v14M6 5l9 7-9 7V5z" fill="currentColor" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                  </svg>
                </button>
              </>
            )}

            {/* Local playlist prev/next */}
            {context?.kind === "local" && context.ids.length > 1 && (
              <>
                <button
                  type="button"
                  className="nf-icon-btn"
                  disabled={busy || context.index <= 0}
                  aria-label="上一部"
                  title="上一部"
                  onClick={() => void openLocalIndex(context.index - 1)}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M6 5v14M18 5l-9 7 9 7V5z" fill="currentColor" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                  </svg>
                </button>
                <button
                  type="button"
                  className="nf-icon-btn"
                  disabled={busy || context.index >= context.ids.length - 1}
                  aria-label="下一部"
                  title="下一部"
                  onClick={() => void openLocalIndex(context.index + 1)}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M18 5v14M6 5l9 7-9 7V5z" fill="currentColor" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                  </svg>
                </button>
              </>
            )}

            {/* Volume Cluster */}
            <div className="nf-volume-cluster">
              <button
                type="button"
                className="nf-icon-btn"
                disabled={!controllable || !preferencesReady}
                aria-label={displayVolume === 0 ? "取消静音" : "静音"}
                title={displayVolume === 0 ? "取消静音 (M)" : "静音 (M)"}
                onClick={() => void toggleMute()}
              >
                {displayVolume === 0 ? (
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M11 5 6 9H2v6h4l5 4V5z" fill="currentColor" />
                    <path d="m22 9-6 6m0-6 6 6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                  </svg>
                ) : displayVolume < 50 ? (
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M11 5 6 9H2v6h4l5 4V5z" fill="currentColor" />
                    <path d="M15.5 8.5a5 5 0 0 1 0 7" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                  </svg>
                ) : (
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M11 5 6 9H2v6h4l5 4V5z" fill="currentColor" />
                    <path d="M15.5 8.5a5 5 0 0 1 0 7m3.2-10.2a9.5 9.5 0 0 1 0 13.4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
                  </svg>
                )}
              </button>
              <div className="nf-volume-slider-wrap">
                <div className="nf-volume-track" aria-hidden="true">
                  <div className="nf-volume-fill" style={{ width: `${displayVolume}%` }} />
                </div>
                <input
                  type="range"
                  className="nf-volume-input"
                  aria-label="音量"
                  min={0}
                  max={100}
                  step={1}
                  disabled={!controllable || !preferencesReady}
                  value={displayVolume}
                  onChange={e => setVolumeDraft(Number(e.target.value))}
                  onPointerUp={() => { if (volumeDraft !== null) void commitVolume(volumeDraft); }}
                  onKeyUp={() => { if (volumeDraft !== null) void commitVolume(volumeDraft); }}
                />
              </div>
            </div>
          </div>

          <div className="nf-controls-right">
            {context?.kind === "local" && context.ids.length > 1 && (
              <label className="nf-autonext-toggle" title="播完当前视频后自动播放下一部">
                <input
                  type="checkbox"
                  checked={autoNext}
                  disabled={!preferencesReady}
                  onChange={e => {
                    const checked = e.target.checked;
                    void persistLocalAutoNext(checked).then(saved => setAutoNext(saved.autoNext)).catch(() => {});
                  }}
                />
                <span>自动连播</span>
              </label>
            )}

            {/* Playback Speed */}
            <div className="nf-popover-anchor">
              <button
                type="button"
                className={"nf-text-btn" + (popover === "speed" ? " is-active" : "")}
                disabled={!controllable || !preferencesReady}
                onClick={() => setPopover(p => (p === "speed" ? "none" : "speed"))}
              >
                {rate === 1 ? "倍速" : `${rate}×`}
              </button>
              {popover === "speed" && (
                <div className="nf-popover nf-speed-popover" role="menu" aria-label="播放倍速">
                  <div className="nf-popover-title">播放速度</div>
                  {SPEEDS.map(speed => (
                    <button
                      key={speed}
                      type="button"
                      role="menuitemradio"
                      aria-checked={rate === speed}
                      className={"nf-menu-item" + (rate === speed ? " is-selected" : "")}
                      onClick={() => {
                        void invokePlayer("set_rate", { rate: speed }).then(ok => {
                          if (ok) setRate(speed);
                        });
                        setPopover("none");
                      }}
                    >
                      <span>{speed === 1 ? "1.0× 正常" : `${speed}×`}</span>
                      {rate === speed && <span className="nf-check">●</span>}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Audio & Subtitles */}
            {hasTracks && mediaTracks && (
              <div className="nf-popover-anchor">
                <button
                  type="button"
                  className={"nf-text-btn" + (popover === "tracks" ? " is-active" : "")}
                  disabled={!controllable || trackBusy}
                  onClick={() => setPopover(p => (p === "tracks" ? "none" : "tracks"))}
                >
                  音轨 / 字幕
                </button>
                {popover === "tracks" && (
                  <div className="nf-popover nf-tracks-popover" role="dialog" aria-label="音轨与字幕">
                    {mediaTracks.audio.length > 0 && (
                      <div className="nf-track-col">
                        <div className="nf-popover-title">音频</div>
                        {mediaTracks.audio.map((track, idx) => {
                          const selected = selectedTrackId(mediaTracks.audio, false) === String(track.id);
                          return (
                            <button
                              key={track.id}
                              type="button"
                              className={"nf-menu-item" + (selected ? " is-selected" : "")}
                              disabled={trackBusy}
                              onClick={() => void selectMediaTrack("audio", String(track.id))}
                            >
                              <span>{trackLabel(track, idx)}</span>
                              {selected && <span className="nf-check">●</span>}
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {mediaTracks.subtitles.length > 0 && (
                      <div className="nf-track-col">
                        <div className="nf-popover-title">字幕</div>
                        <button
                          type="button"
                          className={"nf-menu-item" + (selectedTrackId(mediaTracks.subtitles, true) === "off" ? " is-selected" : "")}
                          disabled={trackBusy}
                          onClick={() => void selectMediaTrack("subtitle", "off")}
                        >
                          <span>关闭字幕</span>
                          {selectedTrackId(mediaTracks.subtitles, true) === "off" && <span className="nf-check">●</span>}
                        </button>
                        {mediaTracks.subtitles.map((track, idx) => {
                          const selected = selectedTrackId(mediaTracks.subtitles, true) === String(track.id);
                          return (
                            <button
                              key={track.id}
                              type="button"
                              className={"nf-menu-item" + (selected ? " is-selected" : "")}
                              disabled={trackBusy}
                              onClick={() => void selectMediaTrack("subtitle", String(track.id))}
                            >
                              <span>{trackLabel(track, idx)}</span>
                              {selected && <span className="nf-check">●</span>}
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {trackError && <p className="nf-track-error">{trackError}</p>}
                  </div>
                )}
              </div>
            )}

            {/* Fullscreen */}
            <button
              type="button"
              className="nf-icon-btn"
              disabled={busy}
              aria-label={fullscreen ? "退出全屏" : "全屏"}
              title={fullscreen ? "退出全屏 (F)" : "全屏 (F)"}
              onClick={() => void toggleFullscreen(!fullscreen)}
            >
              {fullscreen ? (
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M9 4v5H4m11-5v5h5M9 20v-5H4m11 5v-5h5" fill="none" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              ) : (
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M4 9V4h5m6 0h5v5m0 6v5h-5M9 20H4v-5" fill="none" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              )}
            </button>
          </div>
        </div>
      </footer>

      {/* Right Slide-Over Episode Drawer */}
      {context?.kind === "series" && (
        <aside
          className={"nf-drawer" + (drawerOpen ? " is-open" : "")}
          aria-label="选集列表"
          aria-hidden={!drawerOpen}
          onPointerEnter={() => setHoverChrome(true)}
          onPointerLeave={() => setHoverChrome(false)}
        >
          <div className="nf-drawer-header">
            <div>
              <strong>{context.seriesTitle}</strong>
              <small>共 {context.episodes.length} 集</small>
            </div>
            <button type="button" className="nf-icon-btn" aria-label="关闭选集"
              onClick={() => setDrawerOpen(false)}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M6 6l12 12M18 6 6 18" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
              </svg>
            </button>
          </div>
          <div className="nf-drawer-list">
            {context.episodes.map((ep, idx) => {
              const isCurrent = idx === context.index;
              const disabled = busy || ep.playable === false;
              return (
                <button
                  key={ep.id}
                  type="button"
                  disabled={disabled}
                  className={"nf-drawer-ep" + (isCurrent ? " is-current" : "") + (ep.playable === false ? " is-disabled" : "")}
                  onClick={() => void openSeriesEpisode(idx)}
                >
                  <span className="nf-drawer-ep-num">{String(ep.number).padStart(2, "0")}</span>
                  <span className="nf-drawer-ep-copy">
                    <strong>{ep.title}</strong>
                    <small>{ep.playable === false ? "暂不可播" : isCurrent ? "正在播放" : ep.runtimeLabel}</small>
                  </span>
                </button>
              );
            })}
          </div>
        </aside>
      )}

      {/* Shortcuts Dialog */}
      {popover === "shortcuts" && (
        <aside className="nf-shortcuts-modal" role="dialog" aria-label="播放器快捷键">
          <div className="nf-shortcuts-head">
            <strong>快捷键</strong>
            <button type="button" onClick={() => setPopover("none")}>关闭</button>
          </div>
          <dl>
            <div><dt>空格 / K</dt><dd>播放 / 暂停</dd></div>
            <div><dt>← / →</dt><dd>后退 / 前进 5 秒</dd></div>
            <div><dt>J / L</dt><dd>后退 / 前进 10 秒</dd></div>
            <div><dt>↑ / ↓</dt><dd>音量调节</dd></div>
            <div><dt>M</dt><dd>静音切换</dd></div>
            <div><dt>F / 双击</dt><dd>全屏切换</dd></div>
            <div><dt>Esc</dt><dd>退出全屏 / 返回片库</dd></div>
          </dl>
        </aside>
      )}
    </div>
  );
}

createRoot(document.getElementById("overlay-root")!).render(<OverlayApp />);
