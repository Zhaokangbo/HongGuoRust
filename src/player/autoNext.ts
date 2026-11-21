import type { PlayerSnapshot } from "./snapshot";

export interface LocalPlaylist {
  ids: number[];
  index: number;
  origin: string;
  sessionId: number;
}

/**
 * M8-B's auto-advance gate: Ended is emitted by Rust exclusively on libmpv
 * natural EOF. The active native session ID must match the queue entry.
 * Stale Ended snapshots, user Stop (Idle), errors, non-local playback and
 * already-consumed EOF events must never start another file.
 */
export function nextAtNaturalEof(
  playlist: LocalPlaylist | null,
  snapshot: PlayerSnapshot | null,
  enabled: boolean,
  consumedSessionId: number | null,
): number | null {
  if (!enabled || !playlist || !snapshot) return null;
  if (snapshot.state !== "Ended" || snapshot.sessionId !== playlist.sessionId) return null;
  if (playlist.sessionId === consumedSessionId) return null;
  if (!Number.isSafeInteger(playlist.sessionId) || playlist.sessionId <= 0) return null;
  const nextIndex = playlist.index + 1;
  if (nextIndex >= playlist.ids.length || nextIndex < 0) return null;
  return nextIndex;
}
