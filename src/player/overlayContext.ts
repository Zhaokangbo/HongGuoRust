import type { Episode } from "../catalog/types";

export const PLAYER_CONTEXT_EVENT = "player://context";
export const PLAYER_DISMISSED_EVENT = "player://dismissed";

export type PlayerOverlayContext =
  | {
      kind: "series";
      seriesId: string;
      seriesTitle: string;
      episodes: Episode[];
      index: number;
      sessionId: number;
    }
  | {
      kind: "local";
      ids: number[];
      titles: Record<string, string>;
      index: number;
      origin: string;
      sessionId: number;
    }
  | {
      kind: "single";
      title: string;
      sessionId: number;
    };

export function isOverlayContext(value: unknown): value is PlayerOverlayContext {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  if (typeof record.sessionId !== "number" || !Number.isSafeInteger(record.sessionId) || record.sessionId <= 0) {
    return false;
  }
  if (record.kind === "series") {
    return typeof record.seriesId === "string"
      && typeof record.seriesTitle === "string"
      && Array.isArray(record.episodes)
      && typeof record.index === "number"
      && record.index >= 0
      && record.index < record.episodes.length;
  }
  if (record.kind === "local") {
    return Array.isArray(record.ids)
      && typeof record.index === "number"
      && record.index >= 0
      && record.index < record.ids.length
      && typeof record.origin === "string";
  }
  if (record.kind === "single") {
    return typeof record.title === "string";
  }
  return false;
}

export async function presentPlayerOverlay(context: PlayerOverlayContext): Promise<void> {
  const core = window.__TAURI__?.core;
  if (!core) throw new Error("桌面桥接不可用");
  await core.invoke("player_present", { context });
}

export async function updatePlayerOverlayContext(context: PlayerOverlayContext): Promise<void> {
  const core = window.__TAURI__?.core;
  if (!core) return;
  await core.invoke("player_update_context", { context });
}

export async function dismissPlayerOverlay(): Promise<void> {
  const core = window.__TAURI__?.core;
  if (!core) return;
  await core.invoke("player_dismiss");
}

export async function fetchPlayerOverlayContext(): Promise<PlayerOverlayContext | null> {
  const core = window.__TAURI__?.core;
  if (!core) return null;
  const raw = await core.invoke<unknown>("get_player_context");
  return isOverlayContext(raw) ? raw : null;
}
