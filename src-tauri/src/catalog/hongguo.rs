//! HG-1: HongGuo integration entry point, intentionally OFFLINE and gated.
 //! This is NOT an official HongGuo API client or content catalogue.
 //! No API keys, scraping, protected links or inferred playback rights.
use super::{ContentProvider, Episode, MediaTarget, Series, SeriesDetail};
use serde::Deserialize;
use std::collections::HashSet;

const MAX_CONTRACT_BYTES: usize = 256 * 1024;
const MAX_SERIES: usize = 100;
const MAX_EPISODES: usize = 100;
const MAX_TEXT_BYTES: usize = 512;
const MAX_ID_BYTES: usize = 96;

#[derive(Default)]
pub struct HongGuoProvider {
    // Production deliberately starts empty. A future HG-2 authorized backend
    // must establish entitlement independently before supplying real data.
    metadata: Vec<SeriesDetail>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartnerCatalog {
    schema_version: u32,
    series: Vec<PartnerSeries>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartnerSeries {
    id: String,
    title: String,
    genre: String,
    episodes: Vec<PartnerEpisode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartnerEpisode {
    id: String,
    title: String,
    number: usize,
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn safe_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_TEXT_BYTES
        && !value.chars().any(char::is_control)
}

fn validate_metadata_only_contract(bytes: &[u8]) -> Result<Vec<SeriesDetail>, String> {
    if bytes.len() > MAX_CONTRACT_BYTES {
        return Err("hg_metadata_too_large: 元数据超出允许范围".into());
    }
    let catalog: PartnerCatalog = serde_json::from_slice(bytes)
        .map_err(|_| "hg_metadata_invalid: 无法解析元数据合约".to_owned())?;
    if catalog.schema_version != 1 || catalog.series.len() > MAX_SERIES {
        return Err("hg_metadata_schema: 不支持的格式或剧目数量".into());
    }
    let mut ids = HashSet::new();
    let mut normalized = Vec::with_capacity(catalog.series.len());
    for item in catalog.series {
        if !safe_id(&item.id)
            || !ids.insert(item.id.clone())
            || !safe_text(&item.title)
            || !safe_text(&item.genre)
            || item.episodes.is_empty()
            || item.episodes.len() > MAX_EPISODES
        {
            return Err("hg_metadata_series: 剧目数据无效".into());
        }
        let mut episode_ids = HashSet::new();
        let mut numbers = HashSet::new();
        let mut episodes = Vec::with_capacity(item.episodes.len());
        for episode in item.episodes {
            if !safe_id(&episode.id)
                || !episode_ids.insert(episode.id.clone())
                || !numbers.insert(episode.number)
                || !safe_text(&episode.title)
                || episode.number == 0
                || episode.number > MAX_EPISODES
            {
                return Err("hg_metadata_episode: 选集数据无效".into());
            }
            episodes.push(Episode {
                id: episode.id,
                number: episode.number,
                title: episode.title,
                runtime_label: "—".into(),
            });
        }
        episodes.sort_by_key(|episode| episode.number);
        let series = Series {
            id: item.id,
            title: item.title,
            description: "仅用于授权接口适配测试的元数据，不代表已取得片源授权".into(),
            genre: item.genre,
            badge: "接入测试 · 不可播放".into(),
            artwork: "studio".into(),
            episode_count: episodes.len(),
            provider_id: "hongguo".into(),
        };
        normalized.push(SeriesDetail { series, episodes });
    }
    Ok(normalized)
}

impl ContentProvider for HongGuoProvider {
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

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        let query = query.trim().to_lowercase();
        Ok(self
            .metadata
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || entry.series.title.to_lowercase().contains(&query)
                    || entry.series.genre.to_lowercase().contains(&query)
            })
            .map(|entry| entry.series.clone())
            .collect())
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        Ok(self
            .metadata
            .iter()
            .find(|item| item.series.id == series_id)
            .cloned())
    }

    fn resolve_episode(&self, _series_id: &str, _episode_id: &str) -> Result<MediaTarget, String> {
        Err("hg_not_authorized: 红果片源接口及播放授权尚未配置".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"schemaVersion":1,"series":[{"id":"sample-a","title":"合成测试剧","genre":"测试","episodes":[{"id":"ep-02","title":"第二集","number":2},{"id":"ep-01","title":"第一集","number":1}]}]}"#;

    #[test]
    fn production_entry_is_visible_but_never_claims_real_content_or_playback() {
        let provider = HongGuoProvider::default();
        assert_eq!(provider.id(), "hongguo");
        assert!(provider.name().contains("待授权"));
        assert!(!provider.supports_search());
        assert!(!provider.supports_detail());
        assert!(!provider.supports_play());
        assert!(provider.search("").unwrap().is_empty());
        assert!(provider.detail("sample-a").unwrap().is_none());
        assert!(provider.resolve_episode("sample-a", "ep-01").is_err());
    }

    #[test]
    fn offline_partner_metadata_adapter_supports_search_detail_and_namespace() {
        let provider = HongGuoProvider {
            metadata: validate_metadata_only_contract(SAMPLE.as_bytes()).unwrap(),
        };
        assert_eq!(provider.search("合成").unwrap().len(), 1);
        assert!(provider.search("不存在").unwrap().is_empty());
        let detail = provider.detail("sample-a").unwrap().unwrap();
        assert_eq!(detail.series.provider_id, "hongguo");
        assert_eq!(detail.episodes[0].id, "ep-01");
        assert_eq!(detail.episodes[1].number, 2);
        assert_eq!(detail.series.episode_count, 2);
        assert!(provider.resolve_episode("sample-a", "ep-01").is_err());
    }

    #[test]
    fn invalid_and_sensitive_remote_fields_are_rejected_closed() {
        for input in [
            r#"{"schemaVersion":2,"series":[]}"#,
            r#"{"schemaVersion":1,"series":[{"id":"https://host/a","title":"剧目","genre":"测试","episodes":[]}]}"#,
            r#"{"schemaVersion":1,"series":[{"id":"a","title":"剧目","genre":"测试","url":"https://secret.example.com/x.mp4?token=hidden","episodes":[{"id":"e","title":"1","number":1}]}]}"#,
            r#"{"schemaVersion":1,"series":[{"id":"a","title":"剧目","genre":"测试","episodes":[{"id":"e","title":"1","number":1},{"id":"e","title":"2","number":2}]}]}"#,
            r#"{"schemaVersion":1,"series":[{"id":"a","title":"剧目","genre":"测试","episodes":[{"id":"e","title":"1","number":0}]}]}"#,
            "garbled json",
        ] {
            assert!(validate_metadata_only_contract(input.as_bytes()).is_err());
        }
        assert!(validate_metadata_only_contract(&vec![b'x'; MAX_CONTRACT_BYTES + 1]).is_err());
    }

    #[test]
    fn metadata_contract_never_contains_playback_url_or_credentials() {
        let items = validate_metadata_only_contract(SAMPLE.as_bytes()).unwrap();
        let value = serde_json::to_string(&items[0]).unwrap();
        assert!(!value.contains("https://"));
        assert!(!value.contains("accessToken"));
        assert!(!value.contains("playUrl"));
        assert!(!value.contains("sessionKey"));
    }
}
