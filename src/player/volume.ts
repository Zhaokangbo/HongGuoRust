/** Return the volume level used by the player-bar mute toggle. */
export function volumeAfterMute(currentVolume: number, lastAudibleVolume: number): number {
  if (Number.isFinite(currentVolume) && currentVolume > 0) return 0;
  if (!Number.isFinite(lastAudibleVolume)) return 100;
  return Math.min(100, Math.max(1, Math.round(lastAudibleVolume)));
}
