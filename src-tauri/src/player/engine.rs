use super::controls::PlayerControl;
use super::source::MediaSource;
use super::state::Failure;
use libmpv2::{events::Event, mpv_end_file_reason, Mpv};
use std::path::Path;
use std::time::{Duration, Instant};

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

fn native_error(stage: &'static str, e: libmpv2::Error) -> Failure {
    let code = format!("{e:?}");
    // -13 is libmpv's generic loading failure. It is not proof of HTTP 403:
    // the source may be denied, unavailable, return non-media, or fail locally.
    let message = if code.contains("Raw(-13)") {
        "媒体加载失败（libmpv -13）。请检查网络及资源权限，并确认 HTTPS 地址直接返回 MP4，而不是 403/XML/HTML 错误页面。".into()
    } else {
        e.to_string()
    };
    Failure {
        stage,
        code,
        message,
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

impl Engine {
    pub fn load(
        host: isize,
        source: &MediaSource,
        session: u64,
        opened: Instant,
    ) -> Result<Self, Failure> {
        let name = match source {
            MediaSource::Local(path) => {
                local_mp4(path)
                    .map(str::to_owned)
                    .map_err(|message| Failure {
                        stage: "validate",
                        code: "invalid_local_mp4".into(),
                        message,
                    })?
            }
            MediaSource::Https(url) => url.clone(),
        };
        let local = matches!(source, MediaSource::Local(_));
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("wid", host as i64)?;
            init.set_option("load-scripts", false)?;
            init.set_option("ytdl", false)?;
            init.set_option("demuxer", "lavf")?;
            if local {
                init.set_option("demuxer-lavf-format", "mov")?;
                init.set_option("demuxer-lavf-o", "protocol_whitelist=file")?;
            }
            init.set_option("sub-auto", "no")?;
            init.set_option("audio-file-auto", "no")?;
            init.set_option("vo", "gpu-next,gpu,direct3d")?;
            Ok(())
        })
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

    pub fn poll(&mut self) -> Result<Option<Observation>, Failure> {
        match self.mpv.wait_event(0.025) {
            Some(Ok(Event::FileLoaded)) => {
                self.file_loaded = true;
                Ok(Some(Observation::Loaded))
            },
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
                if self.file_loaded
                    && self.cache_checked_at.elapsed() >= Duration::from_millis(200)
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
