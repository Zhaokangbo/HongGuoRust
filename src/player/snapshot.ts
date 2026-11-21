export interface PlayerSnapshot {
  state: "Idle" | "Loading" | "Playing" | "Paused" | "Ended" | "Error";
  filename?: string;
  error?: { stage: string; code: string; message: string };
  revision: number;
  sessionId: number | null;
  appVersion: string;
  positionSec?: number;
  durationSec?: number;
}

declare global {
  interface Window {
    __TAURI__?: {
      core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> };
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
