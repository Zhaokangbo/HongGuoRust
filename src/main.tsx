import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import "./style.css";

type PlayerCommand = "open_local" | "open_https" | "stop" | "set_paused" | "seek_to"
  | "set_rate" | "set_volume" | "set_fullscreen";
const SAMPLE = "https://storage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

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
  const [httpsUrl, setHttpsUrl] = useState("");
  const [seekText, setSeekText] = useState("0");
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const [rate, setRate] = useState(1);
  const [volume, setVolume] = useState(100);
  const [fullscreen, setFullscreen] = useState(false);

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

  const connected = snapshot !== null && !connectionError;
  const active = snapshot?.state === "Playing" || snapshot?.state === "Paused";
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

  return <main className={fullscreen ? "fullscreen" : ""}>
    <header>
      <h1><i aria-hidden="true" />HongGuoRust</h1>
      <span className={connected ? "connection online" : "connection"} role="status">
        {connected ? "已连接" : connectionError ? "连接失败" : "连接中"}
      </span>
    </header>
    <section aria-label="视频画面" />
    <footer className="player-controls">
      <div className="player-actions">
        <button disabled={busy || !connected} onClick={() => void command("open_local")}>打开本地 MP4</button>
        <input aria-label="公开 HTTPS MP4 直链" type="url" spellCheck={false}
          placeholder="粘贴公开 HTTPS MP4 直链" title={SAMPLE}
          value={httpsUrl} onChange={e => setHttpsUrl(e.target.value)} />
        <button disabled={busy || !connected || !httpsUrl.trim()}
          onClick={() => void command("open_https", { url: httpsUrl.trim() })}>播放 HTTPS</button>
        <button disabled={busy || !connected || snapshot?.state === "Idle"}
          onClick={() => void command("stop")}>停止</button>
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
