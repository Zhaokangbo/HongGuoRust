use libmpv2::{events::Event, Mpv};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

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

/// All libmpv API calls, events and Drop belong to this worker thread.
pub fn play(
    host: isize,
    path: &Path,
    cancelled: &AtomicBool,
    mut playing: impl FnMut(),
) -> Result<(), String> {
    let path = local_mp4(path)?;
    let started = Instant::now();
    let mpv = Mpv::with_initializer(|init| {
        init.set_option("wid", host as i64)?;
        init.set_option("load-scripts", false)?;
        init.set_option("ytdl", false)?;
        init.set_option("demuxer", "lavf")?;
        init.set_option("demuxer-lavf-format", "mov")?;
        init.set_option("demuxer-lavf-o", "protocol_whitelist=file")?;
        init.set_option("sub-auto", "no")?;
        init.set_option("audio-file-auto", "no")?;
        init.set_option("vo", "gpu-next,gpu,direct3d")?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    mpv.command("loadfile", &[path])
        .map_err(|e| e.to_string())?;
    while !cancelled.load(Ordering::Acquire) {
        match mpv.wait_event(0.05) {
            Some(Ok(Event::PlaybackRestart)) => {
                println!(
                    "playback-restart {} ms; vo={:?} ao={:?}",
                    started.elapsed().as_millis(),
                    mpv.get_property::<String>("current-vo"),
                    mpv.get_property::<String>("current-ao")
                );
                playing();
            }
            Some(Ok(Event::EndFile(_))) | Some(Ok(Event::Shutdown)) => break,
            Some(Err(error)) => return Err(error.to_string()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
