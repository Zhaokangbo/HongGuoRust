/** M11-C2: persisted metadata-only player preferences from the Rust store. */
export interface PlayerPreferences {
  schemaVersion: number;
  volume: number;
  lastAudibleVolume: number;
  playbackRate: number;
  autoNext: boolean;
}

export function validPlayerPreferences(value: PlayerPreferences | null | undefined): value is PlayerPreferences {
  return value != null
    && value.schemaVersion === 1
    && Number.isFinite(value.volume)
    && value.volume >= 0 && value.volume <= 100
    && Number.isFinite(value.lastAudibleVolume)
    && value.lastAudibleVolume > 0 && value.lastAudibleVolume <= 100
    && Number.isFinite(value.playbackRate)
    && value.playbackRate >= 0.5 && value.playbackRate <= 2
    && typeof value.autoNext === "boolean";
}

export async function readPlayerPreferences(): Promise<PlayerPreferences> {
  const core = window.__TAURI__?.core;
  if (!core) throw new Error("播放器设置只能从桌面程序读取");
  const value = await core.invoke<PlayerPreferences>("get_player_preferences");
  if (!validPlayerPreferences(value)) throw new Error("播放器设置格式或版本不正确");
  return value;
}

export async function persistLocalAutoNext(value: boolean): Promise<PlayerPreferences> {
  const core = window.__TAURI__?.core;
  if (!core) throw new Error("桌面桥接不可用");
  const updated = await core.invoke<PlayerPreferences>("set_player_auto_next", { value });
  if (!validPlayerPreferences(updated)) throw new Error("播放器设置更新响应无效");
  return updated;
}
