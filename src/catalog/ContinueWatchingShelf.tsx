import { useEffect, useRef, useState } from "react";
import { getRecentCatalogEpisodes, listLocalMedia } from "./api";
import { formatEpisodePosition } from "./progress";
import { combineRecentActivity, watchedPercent, type RecentActivityItem } from "./recentActivity";
import { isVisibleProvider } from "./discovery";
import type { LocalMedia, RecentCatalogEpisode } from "./types";
import { CoverImage } from "./CoverImage";

interface Props {
  includeLocal: boolean;
  limit: number;
  busy: boolean;
  refreshKey?: number;
  onPlayProvider: (seriesId: string, episodeId: string) => Promise<boolean>;
  onPlayLocal: (id: number, title?: string) => Promise<boolean>;
}

export function ContinueWatchingShelf({
  includeLocal, limit, busy, refreshKey = 0, onPlayProvider, onPlayLocal,
}: Props) {
  const [providers, setProviders] = useState<RecentCatalogEpisode[]>([]);
  const [locals, setLocals] = useState<LocalMedia[]>([]);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const transitionRef = useRef(false);
  const railRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError("");
    setProviders([]);
    setLocals([]);
    void Promise.allSettled([
      getRecentCatalogEpisodes(24),
      includeLocal ? listLocalMedia() : Promise.resolve([] as LocalMedia[]),
    ]).then(([providerResult, localResult]) => {
      if (!current) return;
      const failures: string[] = [];
      if (providerResult.status === "fulfilled") {
        setProviders(providerResult.value.filter(entry => isVisibleProvider(entry.series.providerId)));
      } else {
        failures.push("剧集观看历史读取失败");
      }
      if (localResult.status === "fulfilled") {
        setLocals(localResult.value);
      } else {
        failures.push("本地观看历史读取失败");
      }
      setError(failures.join("；"));
    }).finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [includeLocal, refreshKey]);

  const items = combineRecentActivity(providers, locals, limit);

  async function play(item: RecentActivityItem) {
    if (transitionRef.current || working || busy) return;
    transitionRef.current = true;
    setWorking(true);
    setError("");
    try {
      const ok = item.kind === "provider"
        ? await onPlayProvider(item.value.series.id, item.value.episode.id)
        : await onPlayLocal(item.value.id, item.value.title);
      if (!ok) setError("暂时无法播放该视频，请检查文件或来源。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      transitionRef.current = false;
      setWorking(false);
    }
  }

  function scrollRail(direction: -1 | 1) {
    const el = railRef.current;
    if (!el) return;
    el.scrollBy({ left: direction * Math.max(360, el.clientWidth * 0.78), behavior: "smooth" });
  }

  if (includeLocal && !loading && items.length === 0 && !error) return null;

  return (
    <section className="nf-rail-section" aria-label={includeLocal ? "继续观看" : "剧集观看历史"}>
      <div className="nf-rail-header">
        <h3>{includeLocal ? "继续观看" : "剧集观看历史"}</h3>
        {items.length > 3 && (
          <div className="nf-rail-arrows">
            <button type="button" aria-label="向左滚动" onClick={() => scrollRail(-1)}>‹</button>
            <button type="button" aria-label="向右滚动" onClick={() => scrollRail(1)}>›</button>
          </div>
        )}
      </div>

      {error && <p className="nf-inline-error" role="alert">{error}</p>}

      {loading ? (
        <div className="nf-rail-skeleton" role="status" aria-label="正在读取观看历史">
          <span /><span /><span /><span />
        </div>
      ) : items.length === 0 ? (
        <p className="nf-empty-hint">暂无剧集观看记录。</p>
      ) : (
        <div className="nf-continue-rail" ref={railRef}>
          {items.map(item => {
            const isProvider = item.kind === "provider";
            const title = isProvider ? item.value.series.title : item.value.title;
            const subtitle = isProvider
              ? `第 ${item.value.episode.number} 集 · ${item.value.episode.title}`
              : "本地视频";
            const position = isProvider ? item.value.progress.positionSec : item.value.positionSec;
            const duration = isProvider ? item.value.progress.durationSec : item.value.durationSec;
            const completed = isProvider ? item.value.progress.completed : item.value.completed;
            const unavailable = !isProvider && !item.value.available;
            const percent = watchedPercent(position, duration, completed);

            return (
              <button
                key={item.key}
                type="button"
                className={"nf-continue-card" + (unavailable ? " is-unavailable" : "")}
                disabled={working || busy || unavailable}
                onClick={() => void play(item)}
              >
                <span className="nf-continue-thumb">
                  <span className="nf-continue-local-bg" aria-hidden="true">
                    {!isProvider && <i>MP4</i>}
                  </span>
                  {isProvider ? (
                    <CoverImage url={item.value.series.coverUrl} seriesId={item.value.series.id} eager />
                  ) : (
                    <CoverImage localId={item.value.id} eager />
                  )}
                  <span className="nf-continue-thumb-shade" aria-hidden="true" />
                  <span className="nf-continue-play-badge" aria-hidden="true">
                    <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
                  </span>
                  {percent !== null && (
                    <span className="nf-card-progress" role="progressbar"
                      aria-label={`${title} 观看进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
                      <i style={{ width: `${percent}%` }} />
                    </span>
                  )}
                </span>
                <span className="nf-continue-meta">
                  <strong title={title}>{title}</strong>
                  <small>
                    {unavailable
                      ? "文件不可用"
                      : completed
                        ? `${subtitle} · 已看完`
                        : `${subtitle} · ${formatEpisodePosition(position)}`}
                  </small>
                </span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}
