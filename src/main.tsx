import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import "./style.css";

type PlayerCommand = "open_local" | "stop" | "set_paused" | "seek_to" | "set_rate" | "set_volume";

function App() {
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [connectionError, setConnectionError] = useState("");
  const [commandError, setCommandError] = useState("");
  const [busy, setBusy] = useState(false);
  const [seekText, setSeekText] = useState("0");
  const [rate, setRate] = useState(1);
  const [volume, setVolume] = useState(100);

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

  async function control(command: PlayerCommand, args?: Record<string, unknown>) {
    setBusy(true);
    setCommandError("");
    try {
      await window.__TAURI__!.core.invoke(command, args);
    } catch (reason) {
      setCommandError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  const connected = snapshot !== null && !connectionError;
  const active = connected && snapshot?.state !== "Idle" && snapshot?.state !== "Error" && snapshot?.state !== "Ended";
  const controllable = active && !busy;
  const error = connectionError || commandError || snapshot?.error?.message;

  return <main>
    <header>
      <h1><i aria-hidden="true" />HongGuoRust</h1>
      <span className={connected ? "connection online" : "connection"} role="status">
        {connected ? "已连接" : connectionError ? "连接失败" : "连接中"}
      </span>
    </header>
    <section aria-label="视频画面" />
    <footer className="player-controls">
      <div className="player-actions">
        <button disabled={busy || !connected} onClick={() => void control("open_local")}>打开本地 MP4</button>
        <button disabled={busy || !connected || snapshot?.state === "Idle"} onClick={() => void control("stop")}>停止</button>
        <span title={snapshot?.filename}>{snapshot?.filename}</span>
        {error && <span role="alert" title={error}>{error}</span>}
      </div>
      <div className="player-transport">
        <button disabled={!controllable} onClick={() => void control("set_paused", { paused: snapshot?.state !== "Paused" })}>
          {snapshot?.state === "Paused" ? "继续" : "暂停"}
        </button>
        <label>跳转(秒)
          <input aria-label="跳转秒数" type="number" min={0} step={1} value={seekText} onChange={event => setSeekText(event.target.value)} />
        </label>
        <button disabled={!controllable || !Number.isFinite(Number(seekText)) || Number(seekText) < 0}
          onClick={() => void control("seek_to", { seconds: Number(seekText) })}>跳转</button>
        <label>倍速
          <select aria-label="播放倍速" value={rate} disabled={!controllable}
            onChange={event => { const next = Number(event.target.value); setRate(next); void control("set_rate", { rate: next }); }}>
            {[0.5, 1, 1.5, 2].map(value => <option value={value} key={value}>{value}×</option>)}
          </select>
        </label>
        <label>音量
          <select aria-label="播放音量" value={volume} disabled={!controllable}
            onChange={event => { const next = Number(event.target.value); setVolume(next); void control("set_volume", { volume: next }); }}>
            {[0, 25, 50, 75, 100].map(value => <option value={value} key={value}>{value}%</option>)}
          </select>
        </label>
        <span className="player-status">{snapshot?.state ?? "—"}</span>
        <span className="player-version">{snapshot?.appVersion ?? "—"}</span>
      </div>
    </footer>
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);
