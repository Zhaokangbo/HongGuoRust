import { useEffect, useState, type FormEvent } from "react";
import {
  catalogErrorMessage, clearOwnedManifest, clearWebDav, configureWebDav,
  getManifestSubscriptionInfo, getWebDavStatus, importOwnedManifest,
  refreshRemoteManifest, subscribeRemoteManifest, syncWebDav,
} from "./api";
import type { ManifestSubscriptionInfo, WebDavStatus } from "./types";

interface Props {
  open: boolean;
  initialTab?: "webdav" | "remote";
  onClose: () => void;
  onChanged: (message: string) => void;
}

function formatSyncTime(ms: number | null): string {
  if (!ms || !Number.isFinite(ms)) return "尚未同步";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit",
  }).format(new Date(ms));
}

export function SourceManagerModal({
  open, initialTab = "webdav", onClose, onChanged,
}: Props) {
  const [tab, setTab] = useState<"webdav" | "remote">(initialTab);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  // Remote manifest state
  const [subInfo, setSubInfo] = useState<ManifestSubscriptionInfo | null>(null);
  const [manifestUrl, setManifestUrl] = useState("");

  // WebDAV state
  const [davStatus, setDavStatus] = useState<WebDavStatus | null>(null);
  const [davName, setDavName] = useState("我的片库");
  const [davUrl, setDavUrl] = useState("");
  const [davUser, setDavUser] = useState("");
  const [davPass, setDavPass] = useState("");

  useEffect(() => {
    if (!open) return;
    setTab(initialTab);
    setError("");
    setNotice("");
    let alive = true;
    void Promise.allSettled([getManifestSubscriptionInfo(), getWebDavStatus()]).then(
      ([subRes, davRes]) => {
        if (!alive) return;
        if (subRes.status === "fulfilled") {
          setSubInfo(subRes.value);
          if (subRes.value.url) setManifestUrl(subRes.value.url);
        }
        if (davRes.status === "fulfilled") {
          setDavStatus(davRes.value);
          if (davRes.value.configured) {
            setDavName(davRes.value.name || "我的片库");
            setDavUrl(davRes.value.baseUrl);
            setDavUser(davRes.value.username);
            setDavPass("");
          }
        }
      },
    );
    return () => { alive = false; };
  }, [open, initialTab]);

  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, busy, onClose]);

  if (!open) return null;

  async function submitWebDav(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy || !davUrl.trim()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const updated = await configureWebDav(davName, davUrl.trim(), davUser, davPass);
      setDavStatus(updated);
      setDavPass("");
      const msg = `WebDAV 已连接：发现 ${updated.seriesCount} 部作品（共 ${updated.episodeCount} 集）`;
      setNotice(msg);
      onChanged(msg);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function handleWebDavSync() {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const updated = await syncWebDav();
      setDavStatus(updated);
      const msg = `WebDAV 同步完成：共 ${updated.seriesCount} 部作品（${updated.episodeCount} 集）`;
      setNotice(msg);
      onChanged(msg);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function handleWebDavClear() {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await clearWebDav();
      const fresh = await getWebDavStatus();
      setDavStatus(fresh);
      setDavUrl("");
      setDavUser("");
      setDavPass("");
      const msg = "已断开并清除 WebDAV 配置";
      setNotice(msg);
      onChanged(msg);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function submitSubscription(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy || !manifestUrl.trim()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const count = await subscribeRemoteManifest(manifestUrl.trim());
      const updated = await getManifestSubscriptionInfo();
      setSubInfo(updated);
      const msg = `远程订阅成功：已同步 ${count} 部作品`;
      setNotice(msg);
      onChanged(msg);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function handleRefreshSubscription() {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const count = await refreshRemoteManifest();
      const updated = await getManifestSubscriptionInfo();
      setSubInfo(updated);
      const msg = `订阅已更新：当前共 ${count} 部作品`;
      setNotice(msg);
      onChanged(msg);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function handleImportLocalJson() {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const count = await importOwnedManifest();
      if (count !== null) {
        const updated = await getManifestSubscriptionInfo();
        setSubInfo(updated);
        const msg = `已导入本地清单：共 ${count} 部作品`;
        setNotice(msg);
        onChanged(msg);
      }
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function handleClearManifest() {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await clearOwnedManifest();
      const updated = await getManifestSubscriptionInfo();
      setSubInfo(updated);
      setManifestUrl("");
      const msg = "已清空剧集清单";
      setNotice(msg);
      onChanged(msg);
    } catch (reason) {
      setError(catalogErrorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="nf-modal-backdrop" role="presentation" onClick={() => { if (!busy) onClose(); }}>
      <div
        className="nf-modal"
        role="dialog"
        aria-modal="true"
        aria-label="片源管理"
        onClick={e => e.stopPropagation()}
      >
        <div className="nf-modal-header">
          <div>
            <h2>片源管理</h2>
            <p>接入你的 WebDAV（Alist / NAS / 网盘）或远程 HTTPS 剧集订阅清单</p>
          </div>
          <button type="button" className="nf-modal-close" aria-label="关闭"
            disabled={busy} onClick={onClose}>✕</button>
        </div>

        <div className="nf-modal-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={tab === "webdav"}
            className={tab === "webdav" ? "is-active" : ""}
            onClick={() => { setTab("webdav"); setError(""); setNotice(""); }}
          >
            WebDAV 媒体库
            {davStatus?.configured && <span className="nf-tab-badge">{davStatus.seriesCount}</span>}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "remote"}
            className={tab === "remote" ? "is-active" : ""}
            onClick={() => { setTab("remote"); setError(""); setNotice(""); }}
          >
            远程订阅清单 (JSON)
            {subInfo && subInfo.seriesCount > 0 && <span className="nf-tab-badge">{subInfo.seriesCount}</span>}
          </button>
        </div>

        {error && <div className="nf-modal-alert is-error" role="alert">{error}</div>}
        {notice && <div className="nf-modal-alert is-ok" role="status">{notice}</div>}

        {tab === "webdav" ? (
          <form className="nf-modal-form" onSubmit={e => void submitWebDav(e)}>
            {davStatus?.configured && (
              <div className="nf-source-status-card">
                <div>
                  <strong>{davStatus.name}</strong>
                  <span>已收录 {davStatus.seriesCount} 部作品 · {davStatus.episodeCount} 个视频 · 上次同步 {formatSyncTime(davStatus.lastSyncedMs)}</span>
                </div>
                <div className="nf-source-status-actions">
                  <button type="button" className="nf-btn-ghost" disabled={busy}
                    onClick={() => void handleWebDavSync()}>
                    {busy ? "同步中…" : "重新同步"}
                  </button>
                  <button type="button" className="nf-local-btn-danger" disabled={busy}
                    onClick={() => void handleWebDavClear()}>
                    断开
                  </button>
                </div>
              </div>
            )}

            <label className="nf-field">
              <span>片库名称</span>
              <input
                type="text"
                className="nf-input-dark"
                placeholder="例如：我的 Alist / 家里 NAS"
                maxLength={64}
                value={davName}
                onChange={e => setDavName(e.target.value)}
              />
            </label>

            <label className="nf-field">
              <span>WebDAV 目录地址</span>
              <input
                type="url"
                className="nf-input-dark"
                required
                spellCheck={false}
                placeholder="例如：http://127.0.0.1:5244/dav/短剧 或 https://nas.example.com/dav/"
                value={davUrl}
                onChange={e => setDavUrl(e.target.value)}
              />
              <small>支持 Alist、群晖/飞牛 NAS、Nextcloud、rclone；子文件夹会自动聚合成带选集的剧集，按自然集数排序。</small>
            </label>

            <div className="nf-field-row">
              <label className="nf-field">
                <span>用户名（可选）</span>
                <input
                  type="text"
                  className="nf-input-dark"
                  placeholder="匿名访问可留空"
                  value={davUser}
                  onChange={e => setDavUser(e.target.value)}
                />
              </label>
              <label className="nf-field">
                <span>密码（可选）</span>
                <input
                  type="password"
                  className="nf-input-dark"
                  placeholder={davStatus?.hasPassword ? "已保存，留空保持不变" : "匿名访问可留空"}
                  value={davPass}
                  onChange={e => setDavPass(e.target.value)}
                />
              </label>
            </div>

            <div className="nf-modal-footer">
              <button type="button" className="nf-btn-ghost" disabled={busy} onClick={onClose}>关闭</button>
              <button type="submit" className="nf-btn-red" disabled={busy || !davUrl.trim()}>
                {busy ? "正在连接并扫描…" : davStatus?.configured ? "保存并重新扫描" : "连接并扫描目录"}
              </button>
            </div>
          </form>
        ) : (
          <form className="nf-modal-form" onSubmit={e => void submitSubscription(e)}>
            {subInfo && subInfo.seriesCount > 0 && (
              <div className="nf-source-status-card">
                <div>
                  <strong>{subInfo.url ? "已订阅远程清单" : "已导入本地清单"}</strong>
                  <span>
                    共 {subInfo.seriesCount} 部作品
                    {subInfo.lastSyncedMs ? ` · 上次同步 ${formatSyncTime(subInfo.lastSyncedMs)}` : ""}
                  </span>
                </div>
                <div className="nf-source-status-actions">
                  {subInfo.url && (
                    <button type="button" className="nf-btn-ghost" disabled={busy}
                      onClick={() => void handleRefreshSubscription()}>
                      {busy ? "更新中…" : "刷新订阅"}
                    </button>
                  )}
                  <button type="button" className="nf-local-btn-danger" disabled={busy}
                    onClick={() => void handleClearManifest()}>
                    清空
                  </button>
                </div>
              </div>
            )}

            <label className="nf-field">
              <span>远程订阅链接（HTTPS JSON）</span>
              <input
                type="url"
                className="nf-input-dark"
                required
                spellCheck={false}
                placeholder="https://example.com/dramas.json"
                value={manifestUrl}
                onChange={e => setManifestUrl(e.target.value)}
              />
              <small>支持 schemaVersion: 1 格式，每部作品可携带标题、简介、题材、coverUrl 封面图及分集 MP4 直链。</small>
            </label>

            <div className="nf-modal-footer nf-modal-footer-split">
              <button type="button" className="nf-btn-ghost" disabled={busy}
                onClick={() => void handleImportLocalJson()}>
                从本地导入 .json 文件
              </button>
              <div className="nf-modal-footer-right">
                <button type="button" className="nf-btn-ghost" disabled={busy} onClick={onClose}>关闭</button>
                <button type="submit" className="nf-btn-red" disabled={busy || !manifestUrl.trim()}>
                  {busy ? "正在拉取…" : "订阅并同步"}
                </button>
              </div>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
