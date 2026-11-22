import { useEffect, useRef, useState } from "react";
import { getRecentCatalogEpisodes, listLocalMedia } from "./api";
import { formatEpisodePosition } from "./progress";
import { combineRecentActivity, watchedPercent, type RecentActivityItem } from "./recentActivity";
import type { LocalMedia, RecentCatalogEpisode } from "./types";
import { CoverImage } from "./CoverImage";

interface Props {
  includeLocal: boolean;
  limit: number;
  busy: boolean;
  refreshKey?: number;
  onPlayProvider: (seriesId: string, episodeId: string) => Promise<boolean>;
  onPlayLocal: (id: number) => Promise<boolean>;
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

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError("");
    setProviders([]);
    setLocals([]);
    // The two independent histories must not mask each other's errors or data.
    void Promise.allSettled([
      getRecentCatalogEpisodes(24),
      includeLocal ? listLocalMedia() : Promise.resolve([] as LocalMedia[]),
    ]).then(([providerResult, localResult]) => {
      if (!current) return;
      const failures: string[] = [];
      if (providerResult.status === "fulfilled") setProviders(providerResult.value);
      else failures.push("剧集观看历史读取失败：" + String(providerResult.reason));
      if (localResult.status === "fulfilled") setLocals(localResult.value);
      else failures.push("本地观看历史读取失败：" + String(localResult.reason));
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
        : await onPlayLocal(item.value.id);
      if (!ok) setError("未能打开该媒体，请刷新或进入详情重试。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      transitionRef.current = false;
      setWorking(false);
    }
  }

  return <section className="recent-activity" aria-label={includeLocal ? "继续观看" : "剧集观看历史"}>
    <div className="recent-heading">
      <h3>{includeLocal ? "继续观看" : "剧集观看历史"}</h3>
      <span>{includeLocal ? "本地 MP4 与合法内容源的最近播放" : "来自演示与自有清单的有效剧集"}</span>
    </div>
    {error && <p className="catalog-progress-error" role="alert">{error}</p>}
    {loading ? <p className="recent-empty" role="status">正在读取观看历史…</p>
      : items.length === 0 ? <p className="recent-empty">
        {includeLocal ? "还没有观看记录。播放任意已接入作品后会显示在这里。"
          : "还没有可用的剧集观看记录。被清空、替换或删除的清单不会显示旧进度。"}
      </p> : <div className="recent-grid">
        {items.map(item => {
          const provider = item.kind === "provider";
          const title = provider ? item.value.series.title : item.value.title;
          const subtitle = provider
            ? "第 " + item.value.episode.number + " 集 · " + item.value.episode.title
            : "本地视频";
          const position = provider ? item.value.progress.positionSec : item.value.positionSec;
          const duration = provider ? item.value.progress.durationSec : item.value.durationSec;
          const completed = provider ? item.value.progress.completed : item.value.completed;
          const unavailable = !provider && !item.value.available;
          const percent = watchedPercent(position, duration, completed);
          return <article key={item.key} className="recent-card">
            <div className="recent-card-art" aria-hidden="true">
              {provider
                ? <CoverImage url={item.value.series.coverUrl} eager />
                : <span className="recent-card-local-art">MP4</span>}
              <span className="recent-card-kind">{provider
                ? item.value.series.providerId === "hgcommunity" ? "社区内容"
                  : item.value.series.providerId === "manifest" ? "自有清单" : "在线目录"
                : "本地视频"}</span>
            </div>
            <div className="recent-card-copy">
              <small>{provider ? "继续观看" : "本地媒体"}</small>
              <strong title={title}>{title}</strong>
              <span>{subtitle}</span>
              <span>{completed ? "已看完" : "上次看到 " + formatEpisodePosition(position)}
                {duration !== null ? " / " + formatEpisodePosition(duration) : ""}</span>
              <time dateTime={new Date(item.lastPlayedMs).toISOString()}>
                {new Date(item.lastPlayedMs).toLocaleString("zh-CN")}
              </time>
              {percent !== null && <div className="recent-progress" role="progressbar"
                aria-label={title + "观看进度"} aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={percent}>
                <div style={{ width: percent + "%" }} />
              </div>}
              {unavailable && <span className="recent-unavailable" role="status">本地文件已移动或不可用</span>}
            </div>
            <button type="button" disabled={working || busy || unavailable}
              onClick={() => void play(item)}>
              {unavailable ? "无法播放" : completed ? "重看" : "继续观看"}
            </button>
          </article>;
        })}
      </div>}
  </section>;
}
