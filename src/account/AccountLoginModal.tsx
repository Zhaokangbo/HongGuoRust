import { useCallback, useEffect, useRef, useState } from "react";
import {
  accountErrorMessage,
  cancelQrLogin,
  getAccountStatus,
  pollQrLogin,
  startQrLogin,
  syncAccount,
  type AccountStatusSnapshot,
  type CloudLibrarySnapshot,
} from "./api";

interface Props {
  open: boolean;
  onClose: () => void;
  onSignedIn: (
    status: AccountStatusSnapshot,
    cloud: CloudLibrarySnapshot | null,
    syncWarning?: string,
  ) => void;
}

type QrStage = "loading" | "waiting" | "scanned" | "syncing" | "expired" | "error";

export function AccountLoginModal({ open, onClose, onSignedIn }: Props) {
  const [stage, setStage] = useState<QrStage>("loading");
  const [qrDataUrl, setQrDataUrl] = useState("");
  const [secondsLeft, setSecondsLeft] = useState(0);
  const [errorText, setErrorText] = useState("");

  const sessionIdRef = useRef<number | null>(null);
  const pollingRef = useRef(false);
  const openRef = useRef(open);
  openRef.current = open;

  const beginSession = useCallback(async () => {
    sessionIdRef.current = null;
    pollingRef.current = false;
    setStage("loading");
    setErrorText("");
    setQrDataUrl("");
    setSecondsLeft(0);
    try {
      const started = await startQrLogin();
      if (!openRef.current) {
        void cancelQrLogin().catch(() => {});
        return;
      }
      sessionIdRef.current = started.sessionId;
      setQrDataUrl(started.qrDataUrl);
      setSecondsLeft(started.expiresInSec);
      setStage("waiting");
    } catch (reason) {
      if (!openRef.current) return;
      setErrorText(accountErrorMessage(reason));
      setStage("error");
    }
  }, []);

  // Start a fresh QR session whenever modal opens; cancel when closed.
  useEffect(() => {
    if (!open) {
      sessionIdRef.current = null;
      void cancelQrLogin().catch(() => {});
      return;
    }
    void beginSession();
    return () => {
      sessionIdRef.current = null;
      void cancelQrLogin().catch(() => {});
    };
  }, [open, beginSession]);

  // Escape to close (unless mid-sync).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && stage !== "syncing") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, stage, onClose]);

  // Countdown timer while waiting / scanned.
  useEffect(() => {
    if (!open || (stage !== "waiting" && stage !== "scanned")) return;
    const timer = window.setInterval(() => {
      setSecondsLeft(prev => {
        if (prev <= 1) {
          setStage("expired");
          sessionIdRef.current = null;
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [open, stage]);

  // Poll official QR status every 2 seconds.
  useEffect(() => {
    if (!open || (stage !== "waiting" && stage !== "scanned")) return;
    const timer = window.setInterval(() => {
      const sid = sessionIdRef.current;
      if (sid === null || pollingRef.current) return;
      pollingRef.current = true;
      void pollQrLogin(sid)
        .then(async res => {
          if (!openRef.current || sessionIdRef.current !== sid) return;
          if (res.status === "scanned") {
            setStage("scanned");
          } else if (res.status === "expired") {
            sessionIdRef.current = null;
            setStage("expired");
          } else if (res.status === "confirmed") {
            sessionIdRef.current = null;
            setStage("syncing");
            let cloud: CloudLibrarySnapshot | null = null;
            let syncWarning: string | undefined;
            try {
              cloud = await syncAccount();
            } catch (syncErr) {
              syncWarning = accountErrorMessage(syncErr);
            }
            const latestStatus = await getAccountStatus().catch(
              () => res.account ?? {
                signedIn: true,
                nickname: cloud?.nickname || "红果用户",
                lastSyncedMs: cloud?.lastSyncedMs ?? null,
                favoritesCount: cloud?.favorites.length ?? 0,
                historyCount: cloud?.history.length ?? 0,
                reservationsCount:
                  (cloud?.onlineReservations.length ?? 0)
                  + (cloud?.upcomingReservations.length ?? 0),
                lastError: syncWarning ?? null,
              },
            );
            if (!openRef.current) return;
            onSignedIn(latestStatus, cloud, syncWarning);
            onClose();
          }
        })
        .catch(reason => {
          if (!openRef.current || sessionIdRef.current !== sid) return;
          // Terminal errors (e.g. account_not_provisioned or redirect rejected) surface cleanly.
          const msg = accountErrorMessage(reason);
          if (msg.includes("业务资料") || msg.includes("不受支持") || msg.includes("未下发")) {
            sessionIdRef.current = null;
            setErrorText(msg);
            setStage("error");
          }
        })
        .finally(() => {
          pollingRef.current = false;
        });
    }, 2000);
    return () => window.clearInterval(timer);
  }, [open, stage, onSignedIn, onClose]);

  if (!open) return null;

  return (
    <div
      className="nf-modal-backdrop"
      role="presentation"
      onClick={() => { if (stage !== "syncing") onClose(); }}
    >
      <div
        className="nf-modal nf-qr-modal"
        role="dialog"
        aria-modal="true"
        aria-label="红果账号扫码登录"
        onClick={e => e.stopPropagation()}
      >
        <div className="nf-modal-header">
          <div>
            <h2>扫码登录红果账号</h2>
            <p>使用手机「红果免费短剧」App 扫一扫，同步追剧收藏、观看进度与预约片单</p>
          </div>
          <button
            type="button"
            className="nf-modal-close"
            aria-label="关闭"
            disabled={stage === "syncing"}
            onClick={onClose}
          >
            ✕
          </button>
        </div>

        <div className="nf-qr-body">
          <div className="nf-qr-box">
            {stage === "loading" ? (
              <div className="nf-qr-placeholder" role="status">
                <span className="nf-red-loader" />
                <span>正在获取官方二维码…</span>
              </div>
            ) : qrDataUrl ? (
              <>
                <img
                  src={qrDataUrl}
                  alt="红果官方登录二维码"
                  className={"nf-qr-img" + (stage === "expired" ? " is-expired" : "")}
                />
                {stage === "expired" && (
                  <div className="nf-qr-mask">
                    <span>二维码已失效</span>
                    <button
                      type="button"
                      className="nf-btn-play nf-qr-refresh-btn"
                      onClick={() => void beginSession()}
                    >
                      刷新二维码
                    </button>
                  </div>
                )}
                {stage === "syncing" && (
                  <div className="nf-qr-mask is-syncing">
                    <span className="nf-red-loader" />
                    <span>正在同步片单…</span>
                  </div>
                )}
              </>
            ) : (
              <div className="nf-qr-placeholder">
                <span>二维码加载失败</span>
              </div>
            )}
          </div>

          <div
            className={
              "nf-qr-status"
              + (stage === "scanned" ? " is-scanned" : "")
              + (stage === "error" ? " is-error" : "")
            }
            role="status"
          >
            {stage === "loading" && "正在连接红果官网获取登录二维码…"}
            {stage === "waiting" && `请打开手机「红果免费短剧」App 扫码确认（约 ${secondsLeft} 秒内有效）`}
            {stage === "scanned" && "已扫描成功！请在手机端点击「确认登录」"}
            {stage === "syncing" && "登录成功，正在拉取你的云端收藏、历史和预约片单…"}
            {stage === "expired" && "二维码已过期，请点击上方按钮刷新后重新扫码"}
            {stage === "error" && (errorText || "获取二维码失败，请检查网络后重试")}
          </div>

          {stage === "error" && (
            <button
              type="button"
              className="nf-btn-secondary"
              onClick={() => void beginSession()}
            >
              重新获取二维码
            </button>
          )}

          <p className="nf-qr-privacy-note">
            登录凭证仅使用 Windows 本机 DPAPI 加密保存，不会明文落盘，也不会随视频播放或弹幕请求发送。
          </p>
        </div>
      </div>
    </div>
  );
}
