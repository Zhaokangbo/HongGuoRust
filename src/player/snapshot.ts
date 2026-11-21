export interface PlayerSnapshot {
  state: "Idle" | "Loading" | "Playing" | "Ended" | "Error";
  filename?: string;
  error?: { stage: string; code: string; message: string };
  revision: number;
  sessionId: number | null;
  appVersion: string;
}

declare global {
  interface Window {
    __TAURI__?: {
      core: { invoke<T>(command: string): Promise<T> };
    };
  }
}

export async function getSnapshot(): Promise<PlayerSnapshot> {
  const core = window.__TAURI__?.core;
  if (!core) {
    throw new Error("请在桌面应用中查看播放器状态。");
  }
  return core.invoke<PlayerSnapshot>("get_snapshot");
}
