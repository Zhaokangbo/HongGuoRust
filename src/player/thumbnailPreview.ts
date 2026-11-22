export interface PreviewFrame {
  sessionId: number;
  timeSec: number;
  dataUrl: string;
}

/** Map the pointer to a preview timestamp without touching playback position. */
export function previewTimeAtPointer(
  clientX: number, left: number, width: number, durationSec: number,
): number | null {
  if (![clientX, left, width, durationSec].every(Number.isFinite)
      || width <= 0 || durationSec <= 0) return null;
  const ratio = Math.min(1, Math.max(0, (clientX - left) / width));
  return ratio * durationSec;
}

export function previewBucket(seconds: number): number | null {
  return Number.isFinite(seconds) && seconds >= 0 && seconds <= 86400
    ? Math.floor(seconds / 5) * 5 : null;
}

/** IPC frames are tied to the exact native session, not the hovered DOM node. */
export function validPreviewFrame(frame: PreviewFrame | null, sessionId: number | null): frame is PreviewFrame {
  return frame !== null
    && sessionId !== null
    && Number.isSafeInteger(frame.sessionId)
    && frame.sessionId === sessionId
    && Number.isFinite(frame.timeSec)
    && frame.timeSec >= 0
    && typeof frame.dataUrl === "string"
    && frame.dataUrl.startsWith("data:image/jpeg;base64,")
    && frame.dataUrl.length < 600_000;
}

export async function fetchPreviewFrame(sessionId: number, seconds: number): Promise<PreviewFrame | null> {
  const core = window.__TAURI__?.core;
  if (!core) return null;
  return core.invoke<PreviewFrame | null>("player_preview_frame", { sessionId, seconds });
}
