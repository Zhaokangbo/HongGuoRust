const frameCache = new Map<string, string | null>();
const inflight = new Map<string, Promise<string | null>>();

function validDataUrl(value: unknown): value is string {
  return typeof value === "string"
    && value.startsWith("data:image/jpeg;base64,")
    && value.length < 700_000;
}

export function getCachedPosterFrame(key: string): string | null | undefined {
  return frameCache.get(key);
}

export function requestLocalPosterFrame(localId: number): Promise<string | null> {
  const key = `local:${localId}`;
  if (frameCache.has(key)) return Promise.resolve(frameCache.get(key) ?? null);
  const existing = inflight.get(key);
  if (existing) return existing;

  const core = window.__TAURI__?.core;
  if (!core) return Promise.resolve(null);

  const task = core
    .invoke<string | null>("local_poster_frame", { id: localId })
    .then(result => {
      const safe = validDataUrl(result) ? result : null;
      frameCache.set(key, safe);
      return safe;
    })
    .catch(() => {
      frameCache.set(key, null);
      return null;
    })
    .finally(() => {
      inflight.delete(key);
    });

  inflight.set(key, task);
  return task;
}

export function requestSeriesPosterFrame(seriesId: string): Promise<string | null> {
  const key = `series:${seriesId}`;
  if (frameCache.has(key)) return Promise.resolve(frameCache.get(key) ?? null);
  const existing = inflight.get(key);
  if (existing) return existing;

  const core = window.__TAURI__?.core;
  if (!core) return Promise.resolve(null);

  const task = core
    .invoke<string | null>("catalog_poster_frame", { seriesId })
    .then(result => {
      const safe = validDataUrl(result) ? result : null;
      frameCache.set(key, safe);
      return safe;
    })
    .catch(() => {
      frameCache.set(key, null);
      return null;
    })
    .finally(() => {
      inflight.delete(key);
    });

  inflight.set(key, task);
  return task;
}
