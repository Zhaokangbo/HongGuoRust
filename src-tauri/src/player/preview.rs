//! M11-B: a separate, best-effort libmpv image-output instance.
//! No access to the primary Engine handle; no main-session seek.
use super::source::MediaSource;
use libmpv2::{events::Event, Mpv};
use serde::Serialize;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STEP_SEC: u64 = 5;
const MAX_REQUEST_SEC: f64 = 24.0 * 60.0 * 60.0;
const MAX_JPEG_BYTES: usize = 384 * 1024;
const MAX_CACHE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CACHE_ITEMS: usize = 12;
const PREVIEW_TIMEOUT: Duration = Duration::from_millis(2400);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFrame {
    pub session_id: u64,
    pub time_sec: f64,
    pub data_url: String,
}

struct Entry {
    session_id: u64,
    bucket: u64,
    frame: PreviewFrame,
    size: usize,
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    size: usize,
}

impl Cache {
    fn get(&mut self, session_id: u64, bucket: u64) -> Option<PreviewFrame> {
        let i = self
            .entries
            .iter()
            .position(|item| item.session_id == session_id && item.bucket == bucket)?;
        let hit = self.entries.remove(i)?;
        let result = hit.frame.clone();
        self.entries.push_back(hit);
        Some(result)
    }

    fn insert(&mut self, session_id: u64, bucket: u64, frame: PreviewFrame) {
        let size = frame.data_url.len();
        if size > MAX_CACHE_BYTES {
            return;
        }
        if let Some(i) = self
            .entries
            .iter()
            .position(|item| item.session_id == session_id && item.bucket == bucket)
        {
            if let Some(old) = self.entries.remove(i) {
                self.size -= old.size;
            }
        }
        self.size += size;
        self.entries.push_back(Entry {
            session_id,
            bucket,
            frame,
            size,
        });
        while self.entries.len() > MAX_CACHE_ITEMS || self.size > MAX_CACHE_BYTES {
            if let Some(old) = self.entries.pop_front() {
                self.size -= old.size;
            }
        }
    }
}

pub struct PreviewService {
    cache: Mutex<Cache>,
    generation: AtomicU64,
}

impl Default for PreviewService {
    fn default() -> Self {
        Self {
            cache: Mutex::new(Cache::default()),
            generation: AtomicU64::new(0),
        }
    }
}

fn time_bucket(seconds: f64) -> Option<u64> {
    if !seconds.is_finite() || !(0.0..=MAX_REQUEST_SEC).contains(&seconds) {
        return None;
    }
    Some((seconds / STEP_SEC as f64).floor() as u64)
}

/// JPEG-only base64 encoding; avoids a new dependency or Cargo.lock drift.
fn encode_base64(input: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(CHARS[(a >> 2) as usize] as char);
        output.push(CHARS[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            CHARS[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            CHARS[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

/// The temporary output is always destroyed, including on errors/cancellation.
struct PreviewDir(PathBuf);

impl PreviewDir {
    fn create() -> Option<Self> {
        let serial = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hongguorust-preview-{}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&path).ok()?;
        Some(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PreviewDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn first_jpeg(dir: &Path) -> Option<Vec<u8>> {
    for file in fs::read_dir(dir).ok()?.flatten() {
        if !file
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg"))
        {
            continue;
        }
        let metadata = file.metadata().ok()?;
        if metadata.len() < 4 || metadata.len() > MAX_JPEG_BYTES as u64 {
            continue;
        }
        let bytes = fs::read(file.path()).ok()?;
        if bytes.starts_with(&[0xff, 0xd8]) && bytes.ends_with(&[0xff, 0xd9]) {
            return Some(bytes);
        }
    }
    None
}

/// The image VO writes exactly one JPEG frame. All native calls happen on the
/// dedicated blocking task for this preview; main playback never waits for it.
fn render_image(
    source: &MediaSource,
    seconds: f64,
    generation: u64,
    latest: &AtomicU64,
) -> Option<Vec<u8>> {
    let folder = PreviewDir::create()?;
    let outdir = folder.path().to_string_lossy().into_owned();
    let seek_start = seconds.to_string();
    let local = matches!(source, MediaSource::Local(_));
    let name = match source {
        MediaSource::Local(path) => path.to_str()?.to_owned(),
        MediaSource::Https(url) => url.clone(),
    };
    let mpv = Mpv::with_initializer(|init| {
        init.set_option("load-scripts", false)?;
        init.set_option("ytdl", false)?;
        init.set_option("demuxer", "lavf")?;
        if local {
            init.set_option("demuxer-lavf-format", "mov")?;
            init.set_option("demuxer-lavf-o", "protocol_whitelist=file")?;
        } else {
            init.set_option("network-timeout", 3_i64)?;
        }
        init.set_option("aid", "no")?;
        init.set_option("sub-auto", "no")?;
        init.set_option("audio-file-auto", "no")?;
        init.set_option("hwdec", "no")?;
        init.set_option("vo", "image")?;
        init.set_option("vo-image-outdir", outdir.as_str())?;
        init.set_option("vo-image-format", "jpg")?;
        init.set_option("vo-image-jpeg-quality", 60_i64)?;
        init.set_option("vf", "scale=320:-2")?;
        init.set_option("frames", 1_i64)?;
        init.set_option("start", seek_start.as_str())?;
        Ok(())
    })
    .ok()?;
    if latest.load(Ordering::Acquire) != generation {
        return None;
    }
    if mpv.command("loadfile", &[name.as_str()]).is_err() {
        return None;
    }
    let started = Instant::now();
    loop {
        if latest.load(Ordering::Acquire) != generation || started.elapsed() >= PREVIEW_TIMEOUT {
            return None;
        }
        match mpv.wait_event(0.05) {
            Some(Ok(Event::EndFile(_))) | Some(Ok(Event::Shutdown)) => break,
            Some(Err(_)) => return None,
            _ => {}
        }
    }
    drop(mpv);
    first_jpeg(folder.path())
}

impl PreviewService {
    /// Invalid input is rejected; unsupported media and timeout degrade to None.
    /// Only one worker renders at a time and stale queued requests return early.
    pub fn frame(
        &self,
        session_id: u64,
        seconds: f64,
        source: &MediaSource,
    ) -> Result<Option<PreviewFrame>, String> {
        let bucket = time_bucket(seconds).ok_or("invalid_preview_position")?;
        if session_id == 0 {
            return Err("invalid_preview_session".into());
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let mut cache = match self.cache.lock() {
            Ok(guard) => guard,
            Err(_) => return Ok(None),
        };
        // An older queued request must not render after a newer hover arrives.
        if self.generation.load(Ordering::Acquire) != generation {
            return Ok(None);
        }
        if let Some(frame) = cache.get(session_id, bucket) {
            return Ok(Some(frame));
        }
        let seconds = (bucket * STEP_SEC) as f64;
        let bytes = match render_image(source, seconds, generation, &self.generation) {
            Some(bytes) if bytes.len() <= MAX_JPEG_BYTES => bytes,
            _ => return Ok(None),
        };
        if self.generation.load(Ordering::Acquire) != generation {
            return Ok(None);
        }
        let frame = PreviewFrame {
            session_id,
            time_sec: seconds,
            data_url: format!("data:image/jpeg;base64,{}", encode_base64(&bytes)),
        };
        cache.insert(session_id, bucket, frame.clone());
        Ok(Some(frame))
    }

    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_is_bounded_and_bucketed() {
        assert_eq!(time_bucket(0.0), Some(0));
        assert_eq!(time_bucket(4.999), Some(0));
        assert_eq!(time_bucket(5.0), Some(1));
        assert_eq!(time_bucket(19.2), Some(3));
        assert_eq!(time_bucket(-1.0), None);
        assert_eq!(time_bucket(f64::NAN), None);
        assert_eq!(time_bucket(f64::INFINITY), None);
        assert_eq!(time_bucket(MAX_REQUEST_SEC + 1.0), None);
    }

    #[test]
    fn base64_encoding_and_cache_eviction_are_bounded() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        let mut cache = Cache::default();
        for bucket in 0..(MAX_CACHE_ITEMS + 5) {
            cache.insert(
                1,
                bucket as u64,
                PreviewFrame {
                    session_id: 1,
                    time_sec: bucket as f64,
                    data_url: format!("data:image/jpeg;base64,{bucket}"),
                },
            );
        }
        assert_eq!(cache.entries.len(), MAX_CACHE_ITEMS);
        assert!(cache.get(1, 0).is_none());
        assert!(cache.get(2, (MAX_CACHE_ITEMS + 4) as u64).is_none());
        assert!(cache.get(1, (MAX_CACHE_ITEMS + 4) as u64).is_some());
        assert!(cache.size <= MAX_CACHE_BYTES);
    }
}
