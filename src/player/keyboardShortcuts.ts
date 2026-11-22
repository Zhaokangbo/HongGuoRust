/**
 * M11-A: pure keyboard mapping. This file has no DOM access or Rust IPC:
 * native session validation stays in the existing PlayerService.
 */
export interface ShortcutKey {
  key: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  repeat?: boolean;
  isComposing?: boolean;
  defaultPrevented?: boolean;
  targetTag?: string;
  targetRole?: string | null;
  contentEditable?: boolean;
}

export interface ShortcutContext {
  catalogOpen: boolean;
  fullscreen: boolean;
  helpOpen: boolean;
  connected: boolean;
  controllable: boolean;
  positionSec: number | null;
  durationSec: number | null;
  volume: number;
  lastAudibleVolume: number;
}

export type PlayerShortcut =
  | { type: "pause" }
  | { type: "seek"; seconds: number }
  | { type: "volume"; value: number }
  | { type: "fullscreen"; value: boolean }
  | { type: "help"; value: boolean };

const FORM_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT", "OPTION"]);

function isTyping(key: ShortcutKey): boolean {
  const tag = key.targetTag?.toUpperCase() ?? "";
  return FORM_TAGS.has(tag) || key.contentEditable === true ||
    key.targetRole?.toLowerCase() === "textbox" ||
    key.targetRole?.toLowerCase() === "searchbox";
}

function validDuration(value: number | null): value is number {
  return value !== null && Number.isFinite(value) && value > 0;
}

export function resolvePlayerShortcut(
  key: ShortcutKey, context: ShortcutContext,
): PlayerShortcut | null {
  if (key.defaultPrevented || key.isComposing || key.repeat ||
      key.ctrlKey || key.altKey || key.metaKey) return null;

  // Preserve the existing F11/Esc controls even with a focused form field.
  if (key.key === "F11" && context.connected) {
    return { type: "fullscreen", value: !context.fullscreen };
  }
  if (key.key === "Escape") {
    if (context.helpOpen) return { type: "help", value: false };
    return context.fullscreen ? { type: "fullscreen", value: false } : null;
  }
  if (context.catalogOpen || isTyping(key)) return null;

  if (key.key === "?" || (key.key === "/" && key.shiftKey)) {
    return { type: "help", value: !context.helpOpen };
  }
  if (!context.connected) return null;
  if (key.key.toLowerCase() === "f") {
    return { type: "fullscreen", value: !context.fullscreen };
  }
  if (context.helpOpen || !context.controllable) return null;

  switch (key.key) {
    case " ":
    case "Spacebar":
    case "k":
    case "K":
      return { type: "pause" };
    case "m":
    case "M":
      return {
        type: "volume",
        value: context.volume === 0
          ? Math.min(100, Math.max(25, context.lastAudibleVolume))
          : 0,
      };
    case "ArrowUp":
    case "ArrowDown": {
      const delta = key.key === "ArrowUp" ? 25 : -25;
      const value = Math.max(0, Math.min(100, context.volume + delta));
      return value === context.volume ? null : { type: "volume", value };
    }
    case "ArrowLeft":
    case "ArrowRight":
    case "j":
    case "J":
    case "l":
    case "L": {
      const duration = context.durationSec;
      if (!validDuration(duration)) return null;
      const delta = key.key === "ArrowLeft" ? -5 : key.key === "ArrowRight" ? 5
        : key.key.toLowerCase() === "j" ? -10 : 10;
      const position = context.positionSec !== null && Number.isFinite(context.positionSec)
        ? context.positionSec : 0;
      const seconds = Math.max(0, Math.min(Math.max(0, duration - 0.25), position + delta));
      return { type: "seek", seconds };
    }
    default:
      return null;
  }
}
