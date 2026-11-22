import { useState } from "react";
import { safeCoverUrl } from "./coverUrl";

interface Props {
  url?: string | null;
  /** Show a non-sensitive diagnostic only for the opt-in community source. */
  diagnostic?: boolean;
}

/** Decorative real cover, with the original CSS gradient as the fallback.
 * Image loading never triggers playback or an SDK media-resolution request.
 */
export function CoverImage({ url, diagnostic = false }: Props) {
  const [failed, setFailed] = useState<string | null>(null);
  const src = safeCoverUrl(url);
  if (!src || src === failed) {
    if (!diagnostic) return null;
    return <span className="catalog-cover-diagnostic" aria-label="封面状态">
      {src ? "封面图片加载失败" : "SDK 未获取到封面"}
    </span>;
  }
  return <img className="catalog-cover-image" src={src} alt=""
    loading="lazy" decoding="async" referrerPolicy="no-referrer"
    onError={() => setFailed(src)} />;
}
