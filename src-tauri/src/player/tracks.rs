//! M11-C1: metadata-only track types. Never export media URLs or filesystem paths.
use serde::Serialize;

const MAX_TRACK_LABEL_CHARS: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Subtitle,
}

impl TrackKind {
    pub fn from_ipc(value: &str) -> Result<Self, &'static str> {
        match value {
            "audio" => Ok(Self::Audio),
            "subtitle" => Ok(Self::Subtitle),
            _ => Err("Unsupported track kind"),
        }
    }

    pub fn mpv_kind(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Subtitle => "sub",
        }
    }

    pub fn mpv_property(self) -> &'static str {
        match self {
            Self::Audio => "aid",
            Self::Subtitle => "sid",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTrack {
    pub id: i64,
    pub selected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackList {
    pub session_id: u64,
    pub audio: Vec<MediaTrack>,
    pub subtitles: Vec<MediaTrack>,
}

/// Embedded metadata is not trusted. Prevent URL/token/path leakage and
/// unbounded or control-character-heavy labels from entering the WebView.
pub fn safe_track_label(value: String) -> Option<String> {
    let clean = value.trim();
    if clean.is_empty()
        || clean.contains("://")
        || clean
            .chars()
            .any(|ch| matches!(ch, '/' | '\\' | '?' | '&' | '='))
        || clean.chars().any(char::is_control)
    {
        return None;
    }
    Some(clean.chars().take(MAX_TRACK_LABEL_CHARS).collect())
}

pub fn track_available(tracks: &[MediaTrack], id: i64) -> bool {
    id > 0 && tracks.iter().any(|track| track.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_kind_is_explicit_and_track_properties_are_scoped() {
        assert_eq!(TrackKind::from_ipc("audio"), Ok(TrackKind::Audio));
        assert_eq!(TrackKind::from_ipc("subtitle"), Ok(TrackKind::Subtitle));
        assert!(TrackKind::from_ipc("video").is_err());
        assert!(TrackKind::from_ipc("sid").is_err());
        assert_eq!(TrackKind::Audio.mpv_property(), "aid");
        assert_eq!(TrackKind::Subtitle.mpv_property(), "sid");
        assert_eq!(TrackKind::Subtitle.mpv_kind(), "sub");
    }

    #[test]
    fn only_existing_positive_ids_are_selectable() {
        let tracks = vec![MediaTrack {
            id: 3,
            selected: true,
            language: None,
            title: None,
        }];
        assert!(track_available(&tracks, 3));
        assert!(!track_available(&tracks, 1));
        assert!(!track_available(&tracks, 0));
        assert!(!track_available(&tracks, -1));
    }

    #[test]
    fn metadata_never_leaks_urls_paths_or_controls() {
        assert_eq!(
            safe_track_label(" 简体中文 ".into()),
            Some("简体中文".into())
        );
        assert!(safe_track_label("https://example.com/secret?token=xx".into()).is_none());
        assert!(safe_track_label("C:\\private\\file".into()).is_none());
        assert!(safe_track_label("foo\nbar".into()).is_none());
        assert!(safe_track_label("id=1".into()).is_none());
        assert_eq!(safe_track_label("a".repeat(200)).unwrap().len(), 72);
    }
}
