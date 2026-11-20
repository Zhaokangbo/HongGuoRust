import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { getSnapshot, type PlayerSnapshot } from "./player/snapshot";
import "./style.css";

function App() {
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = () => getSnapshot().then(
      (result) => {
        if (!active) return;
        setSnapshot(result);
        if (result.probe) timer = setTimeout(refresh, 200);
      },
      (reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      },
    );
    void refresh();
    return () => { active = false; clearTimeout(timer); };
  }, [attempt]);

  const [busy, setBusy] = useState(false);
  async function control(command: "open_local" | "stop") {
    setBusy(true);
    setError("");
    try {
      await window.__TAURI__!.core.invoke(command);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  const connected = snapshot !== null;
  return (
    <main>
      <header>
        <h1><i aria-hidden="true" />HongGuoRust</h1>
        <span className={connected ? "connection online" : "connection"} role="status">
          {connected ? "已连接" : error ? "连接失败" : "连接中"}
        </span>
      </header>
      {snapshot?.probe ? <section aria-label="视频画面" /> : <section aria-label="播放器" className="viewport">
        <div className="screen" aria-hidden="true"><i /></div>
        <h2>{connected ? "等待打开媒体" : error ? "无法读取播放器状态" : "正在连接播放器"}</h2>
        {error && <><p role="alert">{error}</p><button onClick={() => {
          setError("");
          setAttempt((value) => value + 1);
        }}>重试连接</button></>}
      </section>}
      <footer className={snapshot?.probe ? "probe-controls" : undefined}>
        {snapshot?.probe && <div className="probe-actions">
          <button disabled={busy} onClick={() => void control("open_local")}>打开本地 MP4</button>
          <button disabled={busy || snapshot.state === "Idle"} onClick={() => void control("stop")}>停止</button>
          <span title={snapshot.filename}>{snapshot.filename}</span>
          {(error || snapshot.error) && <span role="alert">{error || snapshot.error}</span>}
        </div>}
        <dl><dt>播放器状态</dt><dd>{snapshot?.state ?? "—"}</dd></dl>
        <dl><dt>版本</dt><dd>{snapshot?.appVersion ?? "—"}</dd></dl>
      </footer>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
