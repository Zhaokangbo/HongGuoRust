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
    getSnapshot().then(
      (result) => { if (active) setSnapshot(result); },
      (reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      },
    );
    return () => { active = false; };
  }, [attempt]);

  const connected = snapshot !== null;
  return (
    <main>
      <header>
        <h1><i aria-hidden="true" />HongGuoRust</h1>
        <span className={connected ? "connection online" : "connection"} role="status">
          {connected ? "已连接" : error ? "连接失败" : "连接中"}
        </span>
      </header>
      <section aria-label="播放器" className="viewport">
        <div className="screen" aria-hidden="true"><i /></div>
        <h2>{connected ? "等待打开媒体" : error ? "无法读取播放器状态" : "正在连接播放器"}</h2>
        {error && <><p role="alert">{error}</p><button onClick={() => {
          setError("");
          setAttempt((value) => value + 1);
        }}>重试连接</button></>}
      </section>
      <footer>
        <dl><dt>播放器状态</dt><dd>{snapshot?.state ?? "—"}</dd></dl>
        <dl><dt>版本</dt><dd>{snapshot?.appVersion ?? "—"}</dd></dl>
      </footer>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
