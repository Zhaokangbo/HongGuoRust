use super::controls::PlayerControl;
use super::preferences::PlayerPreferences;
use super::source::MediaSource;
use super::state::Failure;
use super::tracks::{safe_track_label, track_available, MediaTrack, TrackKind, TrackList};
use libmpv2::{events::Event, mpv_end_file_reason, Mpv};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCAL_VIDEO_EXTS: &[&str] = &["mp4", "mkv", "mov", "m4v", "webm", "avi", "flv", "ts"];

pub fn local_mp4(path: &Path) -> Result<&str, String> {
    let name = path.to_str().ok_or("The path is not valid Unicode")?;
    if !path.is_absolute() || name.starts_with(r"\\") || !path.is_file() {
        return Err("Select a file on a local drive".into());
    }
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
    {
        return Err("Select an MP4 file".into());
    }
    Ok(name)
}

/// Validates a local video file opened directly via file picker or Windows "Open with".
pub fn local_video(path: &Path) -> Result<&str, String> {
    let name = path.to_str().ok_or("文件路径包含无法识别的字符")?;
    if !path.is_absolute() || name.starts_with(r"\\") || !path.is_file() {
        return Err("请选择本机磁盘上存在的视频文件".into());
    }
    let supported = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            LOCAL_VIDEO_EXTS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        });
    if !supported {
        return Err("暂不支持该视频文件格式".into());
    }
    Ok(name)
}

fn is_mov_container(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ext.eq_ignore_ascii_case("mp4")
                || ext.eq_ignore_ascii_case("mov")
                || ext.eq_ignore_ascii_case("m4v")
        })
}

pub enum Observation {
    Loaded,
    Restart,
    End,
    CacheWaiting(bool),
    Progress {
        position: Option<f64>,
        duration: Option<f64>,
    },
}

pub struct Engine {
    mpv: Mpv,
    pub session: u64,
    pub opened: Instant,
    progress_checked_at: Instant,
    cache_checked_at: Instant,
    file_loaded: bool,
    cache_waiting: bool,
}

/// Only allow the numerical libmpv error variant into the UI/logging
/// contract. Other Debug/Display errors may include a source URL, query token,
/// local path or untrusted remote response text.
fn safe_mpv_error_code(debug: &str) -> String {
    let Some(raw) = debug.strip_prefix("Raw(").and_then(|s| s.strip_suffix(')')) else {
        return "mpv_native_error".into();
    };
    if raw.is_empty()
        || raw.len() > 7
        || !raw
            .bytes()
            .enumerate()
            .all(|(i, c)| c.is_ascii_digit() || (i == 0 && c == b'-'))
        || raw.parse::<i32>().is_err()
    {
        return "mpv_native_error".into();
    }
    format!("Raw({raw})")
}

fn native_error(stage: &'static str, e: libmpv2::Error) -> Failure {
    let code = safe_mpv_error_code(&format!("{e:?}"));
    // Raw(-13) is a generic libmpv load failure, NOT proof of HTTP 403.
    // Never forward Error::Display text: it may contain signed URL queries,
    // filesystem paths or arbitrary remote diagnostics.
    let message = if code == "Raw(-13)" {
        "媒体加载失败（libmpv -13）。请检查 VPN/代理、网络、资源权限或文件完整性，确认 HTTPS 链接返回 MP4 而非 HTML/XML。"
    } else {
        "播放器原生处理失败。请检查媒体文件完整性、网络连通性和访问权限，再尝试重新打开。"
    };
    Failure {
        stage,
        code,
        message: message.into(),
    }
}

/// A successful EndFile event is not necessarily a natural EOF.
/// libmpv2 reports a non-zero playback error as Err before this function.
fn classify_end_reason(reason: libmpv2::EndFileReason) -> Result<Option<Observation>, Failure> {
    if reason == mpv_end_file_reason::Eof {
        return Ok(Some(Observation::End));
    }
    if reason == mpv_end_file_reason::Redirect {
        // mpv redirects a playlist by continuing with its new entries.
        // Do not tear down its handle as if playback had failed.
        return Ok(None);
    }
    Err(Failure {
        stage: "playback",
        code: format!("unexpected_end_reason_{reason}"),
        message: "Playback stopped without natural EOF (native stop, quit or unsupported redirect)"
            .into(),
    })
}

/// Opt-in native mpv diagnostics for network failures. The log may contain
/// full media URLs (including signed query strings), so never enable it by
/// default or upload the raw file without reviewing secrets first.
fn optional_mpv_diagnostic_path(session: u64) -> Option<String> {
    if std::env::var("HONGGUORUST_MPV_DEBUG").ok().as_deref() != Some("1") {
        return None;
    }
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("HongGuoRust")
        .join("logs");
    if std::fs::create_dir_all(&root).is_err() {
        return None;
    }
    let path = root.join(format!("mpv-{}-{session}.log", std::process::id()));
    Some(path.to_string_lossy().into_owned())
}

impl Engine {
    pub fn load(
        host: isize,
        source: &MediaSource,
        session: u64,
        opened: Instant,
        preferences: PlayerPreferences,
    ) -> Result<Self, Failure> {
        let (name, custom_header, mov_only) = match source {
            MediaSource::Local(path) => (
                local_video(path)
                    .map(str::to_owned)
                    .map_err(|message| Failure {
                        stage: "validate",
                        code: "invalid_local_video".into(),
                        message,
                    })?,
                None,
                is_mov_container(path),
            ),
            MediaSource::Https(url) => (url.clone(), None, false),
            MediaSource::WebDav { url, auth_header } => (
                url.clone(),
                auth_header
                    .as_deref()
                    .map(|auth| format!("Authorization: {auth}")),
                false,
            ),
            MediaSource::LocalStream { url, token_header } => (
                url.clone(),
                Some(format!("X-HGR-Stream-Token: {token_header}")),
                false,
            ),
        };
        let local = matches!(source, MediaSource::Local(_));
        let proxy = if local || matches!(source, MediaSource::LocalStream { .. }) {
            None
        } else {
            crate::catalog::http_client::system_http_proxy(&name)
        };
        let diagnostic = optional_mpv_diagnostic_path(session);
        let mpv = Mpv::with_initializer(|init| {
            if let Some(path) = diagnostic.as_deref() {
                init.set_option("log-file", path)?;
                init.set_option("msg-level", "all=v")?;
            }
            init.set_option("wid", host as i64)?;
            init.set_option("load-scripts", false)?;
            init.set_option("ytdl", false)?;
            init.set_option("demuxer", "lavf")?;
            if local {
                if mov_only {
                    init.set_option("demuxer-lavf-format", "mov")?;
                }
                init.set_option("demuxer-lavf-o", "protocol_whitelist=file")?;
            } else {
                if let Some(proxy_url) = proxy.as_deref() {
                    let _ = init.set_option("http-proxy", proxy_url);
                }
                if let Some(header) = custom_header.as_deref() {
                    init.set_option("http-header-fields", header)?;
                }
            }
            init.set_option("sub-auto", "no")?;
            init.set_option("audio-file-auto", "no")?;
            init.set_option("vo", "gpu-next,gpu,direct3d")?;
            Ok(())
        })
        .map_err(|e| native_error("initialize", e))?;
        if let Some(path) = diagnostic.as_deref() {
            tracing::info!(session_id=session, native_log=%path, "opt-in libmpv diagnostic enabled");
        }
        // Apply saved controls to this new, private libmpv instance before
        // loading media. A new session must not inherit a previous source's
        // temporary track IDs, but does inherit global volume/speed preferences.
        let volume_text = preferences.volume.to_string();
        let rate_text = preferences.playback_rate.to_string();
        mpv.command("set", &["volume", volume_text.as_str()])
            .map_err(|e| native_error("initialize", e))?;
        mpv.command("set", &["speed", rate_text.as_str()])
            .map_err(|e| native_error("initialize", e))?;
        mpv.command("loadfile", &[name.as_str()])
            .map_err(|e| native_error("loadfile", e))?;
        Ok(Self {
            mpv,
            session,
            opened,
            progress_checked_at: Instant::now(),
            cache_checked_at: Instant::now(),
            file_loaded: false,
            cache_waiting: false,
        })
    }

    /// Called only by the worker thread that owns the libmpv handle.
    /// IPC success means the command was accepted, not that a frame was presented.
    pub fn control(&mut self, control: PlayerControl) -> Result<(), Failure> {
        let control = control.validate().map_err(|message| Failure {
            stage: "validate",
            code: "invalid_playback_control".into(),
            message: message.into(),
        })?;
        let arg;
        let (name, args): (&str, Vec<&str>) = match control {
            PlayerControl::Pause(paused) => {
                ("set", vec!["pause", if paused { "yes" } else { "no" }])
            }
            PlayerControl::SeekAbsolute(seconds) => {
                arg = seconds.to_string();
                ("seek", vec![arg.as_str(), "absolute+exact"])
            }
            PlayerControl::SetRate(rate) => {
                arg = rate.to_string();
                ("set", vec!["speed", arg.as_str()])
            }
            PlayerControl::SetVolume(volume) => {
                arg = volume.to_string();
                ("set", vec!["volume", arg.as_str()])
            }
        };
        self.mpv
            .command(name, &args)
            .map_err(|e| native_error("control", e))
    }

    /// Only the owning libmpv worker calls this. Enumerate a bounded number
    /// of tracks using scalar properties instead of exporting raw native nodes.
    /// A Loading session has no usable track metadata yet.
    pub fn tracks(&self) -> TrackList {
        let mut list = TrackList {
            session_id: self.session,
            audio: Vec::new(),
            subtitles: Vec::new(),
        };
        if !self.file_loaded {
            return list;
        }
        let count = self
            .mpv
            .get_property::<i64>("track-list/count")
            .unwrap_or(0)
            .clamp(0, 64);
        for index in 0..count {
            let prefix = format!("track-list/{index}");
            let kind = match self
                .mpv
                .get_property::<String>(&format!("{prefix}/type"))
                .ok()
                .as_deref()
            {
                Some("audio") => TrackKind::Audio,
                Some("sub") => TrackKind::Subtitle,
                _ => continue,
            };
            let id = match self.mpv.get_property::<i64>(&format!("{prefix}/id")) {
                Ok(id) if id > 0 => id,
                _ => continue,
            };
            let track = MediaTrack {
                id,
                selected: self
                    .mpv
                    .get_property::<bool>(&format!("{prefix}/selected"))
                    .unwrap_or(false),
                language: self
                    .mpv
                    .get_property::<String>(&format!("{prefix}/lang"))
                    .ok()
                    .and_then(safe_track_label),
                title: self
                    .mpv
                    .get_property::<String>(&format!("{prefix}/title"))
                    .ok()
                    .and_then(safe_track_label),
            };
            let target = match kind {
                TrackKind::Audio => &mut list.audio,
                TrackKind::Subtitle => &mut list.subtitles,
            };
            if !target.iter().any(|item| item.id == id) {
                target.push(track);
            }
        }
        list
    }

    /// Validate that the ID exists in this session's track list immediately
    /// before changing libmpv aid/sid. Subtitle None means sid=no.
    pub fn select_track(
        &mut self,
        kind: TrackKind,
        track_id: Option<i64>,
    ) -> Result<TrackList, String> {
        if !self.file_loaded {
            return Err("媒体尚未加载完成，无法切换轨道".into());
        }
        let available = self.tracks();
        let candidates = match kind {
            TrackKind::Audio => &available.audio,
            TrackKind::Subtitle => &available.subtitles,
        };
        if let Some(id) = track_id {
            if !track_available(candidates, id) {
                return Err("该轨道不存在或已失效，请刷新轨道列表".into());
            }
        } else if kind == TrackKind::Audio {
            return Err("音频轨必须选择有效轨道".into());
        }
        let id_text = track_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "no".into());
        self.mpv
            .command("set", &[kind.mpv_property(), id_text.as_str()])
            .map_err(|_| "libmpv 拒绝切换轨道".to_owned())?;
        Ok(self.tracks())
    }

    pub fn poll(&mut self) -> Result<Option<Observation>, Failure> {
        match self.mpv.wait_event(0.025) {
            Some(Ok(Event::FileLoaded)) => {
                self.file_loaded = true;
                Ok(Some(Observation::Loaded))
            }
            Some(Ok(Event::PlaybackRestart)) => Ok(Some(Observation::Restart)),
            Some(Ok(Event::EndFile(reason))) => classify_end_reason(reason),
            Some(Ok(Event::Shutdown)) => Err(Failure {
                stage: "playback",
                code: "unexpected_shutdown".into(),
                message: "libmpv shut down unexpectedly".into(),
            }),
            Some(Err(e)) => Err(native_error("playback", e)),
            _ => {
                // Native mpv property distinguishes network cache suspension
                // from the user's own pause request.
                // A remote MP4 may take time to open: do not make repeated
                // synchronous property calls while libmpv is still loading
                // its network stream, or query cache state on every event.
                if self.file_loaded && self.cache_checked_at.elapsed() >= Duration::from_millis(200)
                {
                    self.cache_checked_at = Instant::now();
                    if let Ok(waiting) = self.mpv.get_property::<bool>("paused-for-cache") {
                        if waiting != self.cache_waiting {
                            self.cache_waiting = waiting;
                            return Ok(Some(Observation::CacheWaiting(waiting)));
                        }
                    }
                }
                if self.progress_checked_at.elapsed() >= Duration::from_millis(250) {
                    self.progress_checked_at = Instant::now();
                    let position = self.mpv.get_property::<f64>("time-pos").ok();
                    let duration = self.mpv.get_property::<f64>("duration").ok();
                    if position.is_some() || duration.is_some() {
                        return Ok(Some(Observation::Progress { position, duration }));
                    }
                }
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_natural_eof_reaches_ended() {
        assert!(matches!(
            classify_end_reason(mpv_end_file_reason::Eof),
            Ok(Some(Observation::End))
        ));
        for reason in [
            mpv_end_file_reason::Stop,
            mpv_end_file_reason::Quit,
            mpv_end_file_reason::Error,
        ] {
            assert!(classify_end_reason(reason).is_err());
        }
        assert!(matches!(
            classify_end_reason(mpv_end_file_reason::Redirect),
            Ok(None)
        ));
    }

    #[test]
    fn native_error_codes_never_leak_paths_urls_or_signed_query_parameters() {
        for debug in [
            "NotFound https://cdn.example.com/a.mp4?token=super-secret",
            "Raw(-13) query=https://private.example.com/secret.mp4",
            "Raw(-13)\\nC:\\\\Users\\\\Private\\\\video.mp4",
            "NetworkFailure",
            "Raw(-133333333)",
            "Raw(13.0)",
            "Raw(-13",
        ] {
            assert_eq!(safe_mpv_error_code(debug), "mpv_native_error");
        }
        assert_eq!(safe_mpv_error_code("Raw(-13)"), "Raw(-13)");
        assert_eq!(safe_mpv_error_code("Raw(-1)"), "Raw(-1)");
        assert_eq!(safe_mpv_error_code("Raw(0)"), "Raw(0)");
        assert_eq!(safe_mpv_error_code("Raw(-2147483648)"), "mpv_native_error");
    }

    #[test]
    fn generic_failure_does_not_infer_http_status_from_libmpv_loading_code() {
        let code = safe_mpv_error_code("Raw(-13)");
        assert_eq!(code, "Raw(-13)");
        let unsafe_debug = safe_mpv_error_code("https://host/video.mp4?sig=secret");
        assert_eq!(unsafe_debug, "mpv_native_error");
    }

    #[test]
    fn local_file_selection_keeps_unicode_and_rejects_other_types() {
        let dir = std::env::temp_dir().join(format!("hongguorust-m2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let valid = dir.join("视频 A.MP4");
        let other = dir.join("clip.mkv");
        std::fs::write(&valid, []).unwrap();
        std::fs::write(&other, []).unwrap();
        assert_eq!(local_mp4(&valid).unwrap(), valid.to_str().unwrap());
        assert!(local_mp4(&other).is_err());
        assert!(local_mp4(&dir.join("absent.mp4")).is_err());
        assert!(local_mp4(Path::new("relative.mp4")).is_err());
        assert!(local_mp4(Path::new(r"\\server\media\clip.mp4")).is_err());
        std::fs::remove_file(valid).unwrap();
        std::fs::remove_file(other).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
