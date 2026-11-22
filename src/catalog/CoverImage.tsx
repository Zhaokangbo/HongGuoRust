import { useEffect, useState } from "react";
import { safeCoverUrl } from "./coverUrl";

interface Props {
  url?: string | null;
  /** Show a non-sensitive diagnostic only for the opt-in community source. */
  diagnostic?: boolean;
  /** Visible first-screen images should not wait for WebView's lazy-load heuristic. */
  eager?: boolean;
  /** Reserve high download priority for only a few first-screen posters. */
  priority?: boolean;
}

/** Public remote cover with a gradient fallback. Never print signed CDN URLs.
 * A slow image stays in flight: the timeout is observational, not a retry.
 */
export function CoverImage({ url, diagnostic = false, eager = false, priority = false }: Props) {
  const [failed, setFailed] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<string | null>(null);
  const [slow, setSlow] = useState<string | null>(null);
  const src = safeCoverUrl(url);

  useEffect(() => {
    if (!diagnostic || !src || src === failed || src === loaded) return;
    const timer = window.setTimeout(() => setSlow(src), 8000);
    return () => window.clearTimeout(timer);
  }, [diagnostic, src, failed, loaded]);

  if (!src || src === failed) {
    if (!diagnostic) return null;
    return <span className="catalog-cover-diagnostic" aria-label="封面状态">
      {src ? "封面图片加载失败" : "SDK 未获取到封面"}
    </span>;
  }

  return <>
    <img key={src} className="catalog-cover-image" src={src} alt=""
      loading={eager ? "eager" : "lazy"}
      fetchPriority={priority ? "high" : "auto"}
      decoding="async" referrerPolicy="no-referrer"
      onLoad={() => setLoaded(src)}
      onError={() => setFailed(src)} />
    {diagnostic && slow === src && loaded !== src && <span
      className="catalog-cover-diagnostic" aria-label="封面状态">
      封面请求超过 8 秒
    </span>}
  </>;
}
