import { useEffect, useState } from "react";
import { safeCoverUrl } from "./coverUrl";
import {
  getCachedPosterFrame, requestLocalPosterFrame, requestSeriesPosterFrame,
} from "./posterCache";

interface Props {
  url?: string | null;
  /** When url is absent, extract & cache a real video frame for this series. */
  seriesId?: string;
  /** When url is absent, extract & cache a real video frame for this local library ID. */
  localId?: number;
  /** Kept for compatibility with existing callers; silent fallback in production UI. */
  diagnostic?: boolean;
  /** Visible first-screen images should not wait for WebView's lazy-load heuristic. */
  eager?: boolean;
  /** Reserve high download priority for only a few first-screen posters. */
  priority?: boolean;
}

/** Public remote cover or cached real video frame fallback. Never print signed CDN URLs. */
export function CoverImage({
  url, seriesId, localId, eager = false, priority = false,
}: Props) {
  const remoteSrc = safeCoverUrl(url);
  const [failed, setFailed] = useState<string | null>(null);
  const cacheKey = localId !== undefined ? `local:${localId}` : seriesId ? `series:${seriesId}` : null;
  const [extracted, setExtracted] = useState<string | null>(() =>
    !remoteSrc && cacheKey ? (getCachedPosterFrame(cacheKey) ?? null) : null,
  );

  useEffect(() => {
    if (remoteSrc && remoteSrc !== failed) return;
    if (localId === undefined && !seriesId) {
      setExtracted(null);
      return;
    }
    const key = localId !== undefined ? `local:${localId}` : `series:${seriesId}`;
    const cached = getCachedPosterFrame(key);
    if (cached !== undefined) {
      setExtracted(cached);
      return;
    }
    let alive = true;
    const promise = localId !== undefined
      ? requestLocalPosterFrame(localId)
      : requestSeriesPosterFrame(seriesId!);
    void promise.then(frame => {
      if (alive) setExtracted(frame);
    });
    return () => { alive = false; };
  }, [remoteSrc, failed, localId, seriesId]);

  const src = remoteSrc && remoteSrc !== failed ? remoteSrc : extracted;
  if (!src) return null;

  return (
    <img
      key={src}
      className="nf-cover-image"
      src={src}
      alt=""
      loading={eager ? "eager" : "lazy"}
      fetchPriority={priority ? "high" : "auto"}
      decoding="async"
      referrerPolicy="no-referrer"
      onError={() => {
        if (src === remoteSrc) setFailed(src);
        else setExtracted(null);
      }}
    />
  );
}
