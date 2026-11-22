import { useState } from "react";
import { safeCoverUrl } from "./coverUrl";

interface Props {
  url?: string | null;
}

/** Decorative real cover, with the original CSS gradient as the fallback.
 * Image loading never triggers playback or an SDK media-resolution request.
 */
export function CoverImage({ url }: Props) {
  const [failed, setFailed] = useState<string | null>(null);
  const src = safeCoverUrl(url);
  if (!src || src === failed) return null;
  return <img className="catalog-cover-image" src={src} alt=""
    loading="lazy" decoding="async" referrerPolicy="no-referrer"
    onError={() => setFailed(src)} />;
}
