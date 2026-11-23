/** M11-C1 metadata-only contract from the libmpv worker. No source URLs. */
export interface MediaTrack {
  id: number;
  selected: boolean;
  language?: string;
  title?: string;
}

export interface TrackList {
  sessionId: number;
  audio: MediaTrack[];
  subtitles: MediaTrack[];
}

export type TrackKind = "audio" | "subtitle";

export function usableTrackList(
  response: TrackList | null, sessionId: number | null,
): response is TrackList {
  return response != null
    && sessionId !== null
    && Number.isSafeInteger(response.sessionId)
    && response.sessionId === sessionId
    && Array.isArray(response.audio)
    && Array.isArray(response.subtitles)
    && response.audio.length <= 64
    && response.subtitles.length <= 64
    && [...response.audio, ...response.subtitles].every(t =>
      Number.isSafeInteger(t.id) && t.id > 0 && typeof t.selected === "boolean");
}

export function selectedTrackId(tracks: MediaTrack[], allowOff: boolean): string {
  const selected = tracks.find(track => track.selected);
  return selected ? String(selected.id) : allowOff ? "off" : "";
}

export function trackLabel(track: MediaTrack, index: number): string {
  const parts = [`#${index + 1}`];
  const language = track.language?.trim();
  const title = track.title?.trim();
  if (language) parts.push(language.slice(0, 72));
  if (title) parts.push(title.slice(0, 72));
  return parts.join(" · ");
}

export async function getMediaTracks(sessionId: number): Promise<TrackList> {
  const core = window.__TAURI__?.core;
  if (!core) throw new Error("桌面桥接不可用");
  return core.invoke<TrackList>("player_tracks", { sessionId });
}

export async function changeMediaTrack(
  sessionId: number, kind: TrackKind, trackId: number | null,
): Promise<TrackList> {
  const core = window.__TAURI__?.core;
  if (!core) throw new Error("桌面桥接不可用");
  return core.invoke<TrackList>("player_select_track", { sessionId, kind, trackId });
}
