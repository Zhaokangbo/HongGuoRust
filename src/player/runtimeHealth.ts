/** M12-A. All fields are scalar lifecycle counters, never media metadata. */
export interface RuntimeHealth {
  schemaVersion: number;
  activeSessionId: number | null;
  sessionsOpened: number;
  filesLoaded: number;
  playbackRestarts: number;
  playbackEnded: number;
  playbackFailed: number;
  playbackStopped: number;
  bufferEvents: number;
  bufferTotalMs: number;
  bufferCurrentMs: number | null;
  lastLoadMs: number | null;
  lastPlaybackRestartMs: number | null;
  libmpvReleases: number;
  lastReleaseMs: number | null;
  maxReleaseMs: number;
  lastOutcome: "ended" | "error" | "stopped" | "closed" | null;
}

const countKeys = [
  "sessionsOpened", "filesLoaded", "playbackRestarts", "playbackEnded",
  "playbackFailed", "playbackStopped", "bufferEvents", "bufferTotalMs",
  "libmpvReleases", "maxReleaseMs",
] as const;
const nullableCountKeys = [
  "activeSessionId", "bufferCurrentMs", "lastLoadMs",
  "lastPlaybackRestartMs", "lastReleaseMs",
] as const;

function count(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

export function validRuntimeHealth(value: RuntimeHealth | null | undefined): value is RuntimeHealth {
  return value != null
    && value.schemaVersion === 1
    && countKeys.every(key => count(value[key]))
    && nullableCountKeys.every(key => value[key] === null || count(value[key]))
    && [null, "ended", "error", "stopped", "closed"].includes(value.lastOutcome);
}

export function formatRuntimeHealth(value: RuntimeHealth): string {
  const num = (v: number | null) => v === null ? "—" : String(v);
  return [
    "HongGuoRust 运行诊断 v1（本进程统计，未记录媒体源信息）",
    `会话打开: ${value.sessionsOpened} | 加载成功: ${value.filesLoaded} | 当前会话: ${num(value.activeSessionId)}`,
    `mpv 播放启动/重启信号: ${value.playbackRestarts} | 上次加载耗时: ${num(value.lastLoadMs)} ms | 上次启动信号耗时: ${num(value.lastPlaybackRestartMs)} ms`,
    `自然结束: ${value.playbackEnded} | 失败: ${value.playbackFailed} | 手动停止: ${value.playbackStopped} | 最近结束原因: ${value.lastOutcome ?? "—"}`,
    `缓冲次数: ${value.bufferEvents} | 累计缓冲: ${value.bufferTotalMs} ms | 本次缓冲已持续: ${num(value.bufferCurrentMs)} ms`,
    `libmpv 释放次数: ${value.libmpvReleases} | 最近释放耗时: ${num(value.lastReleaseMs)} ms | 最大释放耗时: ${value.maxReleaseMs} ms`,
    "注意：启动信号不是实际首帧显示时间；CPU/内存/句柄需通过外部工具测量。",
  ].join("\n");
}

export async function getRuntimeHealth(): Promise<RuntimeHealth> {
  const core = window.__TAURI__?.core;
  if (!core) throw new Error("需要在桌面客户端中获取运行诊断");
  const value = await core.invoke<RuntimeHealth>("get_runtime_health");
  if (!validRuntimeHealth(value)) throw new Error("运行诊断数据不受支持");
  return value;
}
