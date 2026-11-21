import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import "./style.css";

function App() {
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [connectionError, setConnectionError] = useState("");
  const [commandError, setCommandError] = useState("");
  const [busy, setBusy] = useState(false);

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

  async function control(command: "open_local" | "stop") {
    setBusy(true);
    setCommandError("");
    try {
      await window.__TAURI__!.core.invoke(command);
    } catch (reason) {
      setCommandError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  const connected = snapshot !== null && !connectionError;
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
      <dl><dt>播放器状态</dt><dd>{snapshot?.state ?? "—"}</dd></dl>
      <dl><dt>版本</dt><dd>{snapshot?.appVersion ?? "—"}</dd></dl>
    </footer>
  </main>;
}
createRoot(document.getElementById("root")!).render(<App />);
