//! HG-1: HongGuo integration boundary, deliberately NOT registered in production.
//!
//! At this stage no official HongGuo desktop catalog/playback permission has been
//! established. The provider can be exercised with an in-process mock but makes
//! NO HTTP calls, reads NO credentials and cannot query real HongGuo episodes.
//! An authorized transport can only be added after platform/API terms are verified.
use super::{ContentProvider, Episode, MediaTarget, Series, SeriesDetail};
use crate::player::source::public_https_url;
use std::collections::HashSet;
use std::sync::Arc;

const MAX_ITEM_COUNT: usize = 50;
const MAX_EPISODES: usize = 200;
const MAX_ID_BYTES: usize = 96;
const MAX_TITLE_CHARS: usize = 100;

/// Contract is testable without registering HongGuo in the application's
/// production source list, and without adding any network dependencies.
pub trait ApprovedCatalogTransport: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<ApprovedSeries>, String>;
    fn detail(&self, id: &str) -> Result<Option<ApprovedSeriesDetail>, String>;
    fn play(&self, id: &str, episode_id: &str) -> Result<String, String>;
}

#[derive(Clone, Debug)]
pub struct ApprovedSeries {
    pub id: String,
    pub title: String,
    pub description: String,
    pub genre: String,
    pub episodes: usize,
}

#[derive(Clone, Debug)]
pub struct ApprovedSeriesDetail {
    pub series: ApprovedSeries,
    pub episodes: Vec<ApprovedEpisode>,
}

#[derive(Clone, Debug)]
pub struct ApprovedEpisode {
    pub id: String,
    pub number: usize,
    pub title: String,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn valid_text(text: &str, max_chars: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max_chars && !text.chars().any(char::is_control)
}

fn check_series(item: &ApprovedSeries) -> Result<(), String> {
    if !valid_id(&item.id)
        || !valid_text(&item.title, MAX_TITLE_CHARS)
        || !valid_text(&item.description, 500)
        || !valid_text(&item.genre, 40)
        || item.episodes == 0
        || item.episodes > MAX_EPISODES
    {
        return Err("hg_invalid_catalog: Provider data is not valid".into());
    }
    Ok(())
}

fn safe_series(item: ApprovedSeries) -> Result<Series, String> {
    check_series(&item)?;
    Ok(Series {
        id: item.id,
        title: item.title,
        description: item.description,
        genre: item.genre,
        badge: "授权样例（非红果真实内容）".into(),
        artwork: "studio".into(),
        cover_url: None,
        episode_count: item.episodes,
        provider_id: "hongguo".into(),
    })
}

/// URLs returned by the future authorized partner must be validated against
/// the explicitly agreed media hostname; public_https_url also rejects local
/// IPs, userinfo, unsupported schemes/ports and non-MP4 targets.
/// Important: a future network implementation must additionally pin DNS and
/// validate every redirect target to avoid DNS-rebinding/SSRF.
pub fn allowed_media_url(value: &str, approved_host: &str) -> Result<String, String> {
    let verified = public_https_url(value).map_err(|_| "hg_invalid_media_url")?;
    if !valid_public_host(approved_host) {
        return Err("hg_unapproved_host: Invalid approved hostname".into());
    }
    let authority = verified[8..].split(['/', '?']).next().unwrap_or("");
    let origin = authority
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if origin != approved_host.to_ascii_lowercase() {
        return Err("hg_unapproved_host: Media domain does not match agreement".into());
    }
    Ok(verified)
}

fn valid_public_host(host: &str) -> bool {
    if host.len() > 253 || host.is_empty() || host.contains(':') {
        return false;
    }
    let labels: Vec<_> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        && labels.last().is_some_and(|suffix| {
            suffix.len() >= 2 && suffix.bytes().all(|c| c.is_ascii_alphabetic())
        })
        && !["local", "internal", "localhost", "test", "invalid"]
            .contains(&labels.last().unwrap().to_ascii_lowercase().as_str())
}

/// Visible production source: no partner API or desktop content rights have
/// been granted to this application. Never resolves a real media target.
/// The approved mock transport below remains unregistered pending entitlement.
#[derive(Default)]
pub struct HongGuoUnavailableProvider;

impl ContentProvider for HongGuoUnavailableProvider {
    fn id(&self) -> &'static str {
        "hongguo"
    }

    fn name(&self) -> &'static str {
        "红果接入（待授权）"
    }

    fn supports_search(&self) -> bool {
        false
    }

    fn supports_detail(&self) -> bool {
        false
    }

    fn supports_play(&self) -> bool {
        false
    }

    fn search(&self, _query: &str) -> Result<Vec<Series>, String> {
        Ok(Vec::new())
    }

    fn detail(&self, _series_id: &str) -> Result<Option<SeriesDetail>, String> {
        Ok(None)
    }

    fn resolve_episode(&self, _series_id: &str, _episode_id: &str) -> Result<MediaTarget, String> {
        Err("hg_not_authorized: 红果播放接口尚未授权".into())
    }
}

pub struct HongGuoAuthorizedProvider {
    transport: Arc<dyn ApprovedCatalogTransport>,
    media_host: String,
}

impl HongGuoAuthorizedProvider {
    /// Does not claim a platform partnership. Only called by tests today.
    /// Registering with the production catalog requires separately verified
    /// distribution/API authorization and network transport security review.
    pub fn for_approved_transport(
        transport: Arc<dyn ApprovedCatalogTransport>,
        media_host: String,
    ) -> Result<Self, String> {
        if !valid_public_host(&media_host) {
            return Err("hg_unapproved_host: Invalid approved hostname".into());
        }
        Ok(Self {
            transport,
            media_host,
        })
    }
}

impl ContentProvider for HongGuoAuthorizedProvider {
    fn id(&self) -> &'static str {
        "hongguo"
    }

    fn name(&self) -> &'static str {
        "红果适配接口（尚未启用）"
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        if query.len() > 128 || query.chars().any(char::is_control) {
            return Err("hg_invalid_query: Invalid query".into());
        }
        let items = self
            .transport
            .search(query)
            .map_err(|_| "hg_search_unavailable")?;
        if items.len() > MAX_ITEM_COUNT {
            return Err("hg_invalid_catalog: Too many results".into());
        }
        let mut seen = HashSet::new();
        items
            .into_iter()
            .map(|item| {
                if !seen.insert(item.id.clone()) {
                    return Err("hg_invalid_catalog: Duplicate series ID".into());
                }
                safe_series(item)
            })
            .collect()
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        if !valid_id(series_id) {
            return Err("hg_invalid_series: Invalid series ID".into());
        }
        let details = self
            .transport
            .detail(series_id)
            .map_err(|_| "hg_detail_unavailable")?;
        let Some(details) = details else {
            return Ok(None);
        };
        if details.series.id != series_id || details.episodes.len() != details.series.episodes {
            return Err("hg_invalid_catalog: Episode count mismatch".into());
        }
        let series = safe_series(details.series)?;
        let mut ids = HashSet::new();
        let mut numbers = HashSet::new();
        let mut episodes = Vec::with_capacity(details.episodes.len());
        for item in details.episodes {
            if !valid_id(&item.id)
                || item.number == 0
                || item.number > MAX_EPISODES
                || !valid_text(&item.title, MAX_TITLE_CHARS)
                || !ids.insert(item.id.clone())
                || !numbers.insert(item.number)
            {
                return Err("hg_invalid_catalog: Invalid episode".into());
            }
            episodes.push(Episode {
                id: item.id,
                number: item.number,
                title: item.title,
                runtime_label: "—".into(),
                playable: true,
            });
        }
        episodes.sort_by_key(|episode| episode.number);
        Ok(Some(SeriesDetail { series, episodes }))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        if !valid_id(series_id) || !valid_id(episode_id) {
            return Err("hg_invalid_episode: Invalid episode ID".into());
        }
        let details = self
            .detail(series_id)?
            .ok_or("hg_unknown_series: Series not found")?;
        if !details.episodes.iter().any(|item| item.id == episode_id) {
            return Err("hg_unknown_episode: Episode not found".into());
        }
        // Never allow callers to supply their own URL as an episode ID. Only
        // the approved transport can resolve one, and the host is rechecked.
        let url = self
            .transport
            .play(series_id, episode_id)
            .map_err(|_| "hg_play_unavailable")?;
        allowed_media_url(&url, &self.media_host).map(MediaTarget::Https)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_placeholder_never_exposes_authorized_mock_or_a_playable_target() {
        let slot = HongGuoUnavailableProvider;
        assert_eq!(slot.id(), "hongguo");
        assert!(slot.name().contains("待授权"));
        assert!(!slot.supports_search());
        assert!(!slot.supports_detail());
        assert!(!slot.supports_play());
        assert!(slot.search("").unwrap().is_empty());
        assert!(slot.detail("synthetic-1").unwrap().is_none());
        assert!(slot.resolve_episode("synthetic-1", "ep-1").is_err());
    }

    struct MockAuthorizedTransport;

    impl ApprovedCatalogTransport for MockAuthorizedTransport {
        fn search(&self, _query: &str) -> Result<Vec<ApprovedSeries>, String> {
            Ok(vec![ApprovedSeries {
                id: "synthetic-1".into(),
                title: "虚构测试作品".into(),
                description: "仅用于验证协议适配结构".into(),
                genre: "测试".into(),
                episodes: 2,
            }])
        }

        fn detail(&self, id: &str) -> Result<Option<ApprovedSeriesDetail>, String> {
            if id != "synthetic-1" {
                return Ok(None);
            }
            Ok(Some(ApprovedSeriesDetail {
                series: self.search("")?.remove(0),
                episodes: vec![
                    ApprovedEpisode {
                        id: "ep-2".into(),
                        number: 2,
                        title: "第二集".into(),
                    },
                    ApprovedEpisode {
                        id: "ep-1".into(),
                        number: 1,
                        title: "第一集".into(),
                    },
                ],
            }))
        }

        fn play(&self, id: &str, episode_id: &str) -> Result<String, String> {
            if id != "synthetic-1" || episode_id != "ep-1" {
                return Err("not found".into());
            }
            Ok("https://media.example.org/authorized-demo.mp4".into())
        }
    }

    #[test]
    fn synthetic_catalog_search_detail_episode_and_resolution() {
        let provider = HongGuoAuthorizedProvider::for_approved_transport(
            Arc::new(MockAuthorizedTransport),
            "media.example.org".into(),
        )
        .unwrap();
        assert_eq!(provider.id(), "hongguo");
        let items = provider.search("").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "synthetic-1");
        assert_eq!(
            provider.detail("synthetic-1").unwrap().unwrap().episodes[0].id,
            "ep-1"
        );
        assert_eq!(
            provider.resolve_episode("synthetic-1", "ep-1").unwrap(),
            MediaTarget::Https("https://media.example.org/authorized-demo.mp4".into())
        );
        assert!(provider.resolve_episode("synthetic-1", "ep-99").is_err());
        assert!(provider
            .resolve_episode("https://example.org/", "ep-1")
            .is_err());
    }

    #[test]
    fn approved_hostname_blocks_cross_domain_and_private_media() {
        for url in [
            "http://media.example.org/a.mp4",
            "https://media.example.org.evil.net/a.mp4",
            "https://127.0.0.1/a.mp4",
            "https://user:pass@media.example.org/a.mp4",
            "https://media.example.org:8443/a.mp4",
            "https://media.example.org/a.m3u8",
            "https://media.example.org/a.mp4#frag",
        ] {
            assert!(allowed_media_url(url, "media.example.org").is_err());
        }
        assert!(allowed_media_url("https://media.example.org/a.mp4", "localhost").is_err());
        assert!(allowed_media_url("https://media.example.org/a.mp4", "media.example.org").is_ok());
    }

    #[test]
    fn malformed_transport_data_fails_closed_without_creating_user_content() {
        struct Malformed;
        impl ApprovedCatalogTransport for Malformed {
            fn search(&self, _: &str) -> Result<Vec<ApprovedSeries>, String> {
                Ok(vec![ApprovedSeries {
                    id: "../secret".into(),
                    title: "bad".into(),
                    description: "bad".into(),
                    genre: "test".into(),
                    episodes: 1,
                }])
            }
            fn detail(&self, _: &str) -> Result<Option<ApprovedSeriesDetail>, String> {
                Ok(None)
            }
            fn play(&self, _: &str, _: &str) -> Result<String, String> {
                Ok("https://evil.example.org/video.mp4".into())
            }
        }
        let provider = HongGuoAuthorizedProvider::for_approved_transport(
            Arc::new(Malformed),
            "media.example.org".into(),
        )
        .unwrap();
        assert!(provider.search("").is_err());
        assert!(provider.detail("../secret").is_err());
        assert!(provider.resolve_episode("test", "ep-1").is_err());
    }
}
