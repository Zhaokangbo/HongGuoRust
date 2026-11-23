import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { CatalogView } from "./catalog/CatalogView";
import type { SeriesDetail } from "./catalog/types";
import { createEpisodeQueue } from "./player/episodeQueue";
import {
  PLAYER_DISMISSED_EVENT, presentPlayerOverlay, type PlayerOverlayContext,
} from "./player/overlayContext";
import "./style.css";

function App() {
  const [playerError, setPlayerError] = useState("");
  const [playerActive, setPlayerActive] = useState(false);
  const [refreshSignal, setRefreshSignal] = useState(0);
  const transitionRef = useRef(false);
  const startupCheckedRef = useRef(false);

  // Refresh watch progress whenever the user returns from the overlay player.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | null = null;
    const listen = window.__TAURI__?.event?.listen;
    if (listen) {
      void listen(PLAYER_DISMISSED_EVENT, () => {
        if (alive) {
          setPlayerActive(false);
          setRefreshSignal(v => v + 1);
        }
      }).then(fn => {
        if (alive) unlisten = fn;
        else fn();
      }).catch(() => {});
    }
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  async function launchOverlay(context: PlayerOverlayContext) {
    setPlayerActive(true);
    try {
      await presentPlayerOverlay(context);
    } catch (reason) {
      setPlayerActive(false);
      throw reason;
    }
  }

  // Auto-play a local video file passed via Windows Explorer "Open with" / CLI argument.
  useEffect(() => {
    if (startupCheckedRef.current) return;
    startupCheckedRef.current = true;
    const core = window.__TAURI__?.core;
    if (!core) return;
    void core
      .invoke<{ sessionId: number; title: string } | null>("open_startup_media")
      .then(result => {
        if (!result || !Number.isSafeInteger(result.sessionId) || result.sessionId <= 0) return;
        return launchOverlay({
          kind: "single",
          title: result.title,
          sessionId: result.sessionId,
        });
      })
      .catch(reason => {
        setPlayerError(String(reason));
      });
  }, []);

  async function playFromCatalog(
    seriesId: string, episodeId: string, detail: SeriesDetail,
  ): Promise<boolean> {
    if (transitionRef.current) return false;
    if (detail.series.id !== seriesId || !detail.episodes.some(ep => ep.id === episodeId)) {
      setPlayerError("选集数据已过期，请重新进入详情");
      return false;
    }
    let queue;
    try {
      queue = createEpisodeQueue(detail, episodeId, 1);
    } catch (reason) {
      setPlayerError(String(reason));
      return false;
    }
    transitionRef.current = true;
    setPlayerError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("桌面桥接不可用");
      const sessionId = await core.invoke<number>("catalog_play_episode", { seriesId, episodeId });
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("播放器启动失败");
      }
      const context: PlayerOverlayContext = {
        kind: "series",
        seriesId: queue.seriesId,
        seriesTitle: queue.seriesTitle,
        episodes: queue.episodes,
        index: queue.index,
        sessionId,
      };
      await launchOverlay(context);
      return true;
    } catch (reason) {
      setPlayerError(String(reason));
      return false;
    } finally {
      transitionRef.current = false;
    }
  }

  async function playFromLocal(
    id: number, orderedIds: number[], origin: string, titles?: Record<string, string>,
  ): Promise<boolean> {
    if (transitionRef.current) return false;
    const index = orderedIds.indexOf(id);
    const ids = index < 0 ? [id] : [...orderedIds];
    const resolvedIndex = index < 0 ? 0 : index;
    transitionRef.current = true;
    setPlayerError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("桌面桥接不可用");
      const sessionId = await core.invoke<number>("library_play", { id });
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("视频打开失败");
      }
      const context: PlayerOverlayContext = {
        kind: "local",
        ids,
        titles: titles ?? {},
        index: resolvedIndex,
        origin,
        sessionId,
      };
      await launchOverlay(context);
      return true;
    } catch (reason) {
      setPlayerError(String(reason));
      return false;
    } finally {
      transitionRef.current = false;
    }
  }

  async function openLocalFile(): Promise<void> {
    if (transitionRef.current) return;
    transitionRef.current = true;
    setPlayerError("");
    try {
      const core = window.__TAURI__?.core;
      if (!core) throw new Error("桌面桥接不可用");
      const sessionId = await core.invoke<number | null>("open_local");
      if (sessionId === null) return;
      if (!Number.isSafeInteger(sessionId) || sessionId <= 0) {
        throw new Error("本地视频打开失败");
      }
      const context: PlayerOverlayContext = {
        kind: "single",
        title: "本地视频",
        sessionId,
      };
      await launchOverlay(context);
    } catch (reason) {
      setPlayerError(String(reason));
    } finally {
      transitionRef.current = false;
    }
  }

  return (
    <>
      <CatalogView
        onPlay={playFromCatalog}
        onPlayLocal={playFromLocal}
        onOpenLocalFile={openLocalFile}
        playerError={playerError}
        onClearPlayerError={() => setPlayerError("")}
        refreshSignal={refreshSignal}
      />
      {playerActive && <div className="nf-player-blackout" aria-hidden="true" />}
    </>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
