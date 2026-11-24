import type { Series } from "../catalog/types";

function desktopInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = window.__TAURI__?.core;
  if (!core) return Promise.reject(new Error("账号功能仅能在 HongGuoRust 桌面客户端中使用"));
  return core.invoke<T>(command, args);
}

export interface AccountStatusSnapshot {
  signedIn: boolean;
  nickname: string;
  lastSyncedMs: number | null;
  favoritesCount: number;
  historyCount: number;
  reservationsCount: number;
  lastError: string | null;
}

export interface QrLoginStart {
  sessionId: number;
  qrDataUrl: string;
  expiresInSec: number;
}

export interface QrPollSnapshot {
  status: "waiting" | "scanned" | "confirmed" | "expired" | string;
  account: AccountStatusSnapshot | null;
}

export interface CloudFavorite {
  series: Series;
  addedAtMs: number;
}

export interface CloudHistoryEntry {
  series: Series;
  episodeId: string;
  episodeNumber: number;
  episodeTitle: string;
  positionSec: number;
  durationSec: number | null;
  updatedAtMs: number;
}

export interface CloudReservation {
  series: Series;
  online: boolean;
  publishAtMs: number;
  publishText: string;
}

export interface CloudLibrarySnapshot {
  signedIn: boolean;
  nickname: string;
  lastSyncedMs: number | null;
  favorites: CloudFavorite[];
  history: CloudHistoryEntry[];
  onlineReservations: CloudReservation[];
  upcomingReservations: CloudReservation[];
  lastError: string | null;
}

export function getAccountStatus(): Promise<AccountStatusSnapshot> {
  return desktopInvoke<AccountStatusSnapshot>("account_status");
}

export function startQrLogin(): Promise<QrLoginStart> {
  return desktopInvoke<QrLoginStart>("account_qr_start");
}

export function pollQrLogin(sessionId: number): Promise<QrPollSnapshot> {
  return desktopInvoke<QrPollSnapshot>("account_qr_poll", { sessionId });
}

export function cancelQrLogin(): Promise<void> {
  return desktopInvoke<void>("account_qr_cancel");
}

export function syncAccount(): Promise<CloudLibrarySnapshot> {
  return desktopInvoke<CloudLibrarySnapshot>("account_sync");
}

export function getCloudLibrary(): Promise<CloudLibrarySnapshot> {
  return desktopInvoke<CloudLibrarySnapshot>("account_cloud_library");
}

export function logoutAccount(): Promise<AccountStatusSnapshot> {
  return desktopInvoke<AccountStatusSnapshot>("account_logout");
}

export function accountErrorMessage(reason: unknown): string {
  const raw = reason instanceof Error ? reason.message : String(reason ?? "");
  const cleaned = raw.replace(/^[a-z0-9_]+:\s*/i, "").trim();
  return cleaned || "账号操作失败，请稍后重试。";
}
