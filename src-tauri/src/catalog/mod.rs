//! Content-provider boundary for M6. No scraping and no third-party credentials.
//! LocalDemoProvider uses one known public 20-second test MP4 for every episode.

use serde::Serialize;

const PREVIEW_MP4: &str = "https://samplelib.com/lib/preview/mp4/sample-20s.mp4";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub id: String,
    pub title: String,
    pub description: String,
    pub genre: String,
    pub badge: String,
    pub artwork: String,
    pub episode_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub number: usize,
    pub title: String,
    pub runtime_label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDetail {
    pub series: Series,
    pub episodes: Vec<Episode>,
}

pub trait ContentProvider: Send + Sync {
    fn search(&self, query: &str) -> Vec<Series>;
    fn detail(&self, series_id: &str) -> Option<SeriesDetail>;
    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<String, String>;
}

#[derive(Default)]
pub struct LocalDemoProvider;

struct DemoDefinition {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    genre: &'static str,
    badge: &'static str,
    artwork: &'static str,
    episodes: usize,
}

const DEMOS: [DemoDefinition; 3] = [
    DemoDefinition {
        id: "forest-demo",
        title: "林间光影",
        description: "演示条目：用于验证内容浏览、选集和播放切换。剧集重复使用同一个公开测试视频。",
        genre: "自然影像",
        badge: "公开测试素材",
        artwork: "forest",
        episodes: 3,
    },
    DemoDefinition {
        id: "city-demo",
        title: "城市片刻",
        description: "演示条目：模拟短剧详情与搜索，不代表任何真实作品的授权片源。",
        genre: "城市记录",
        badge: "仅供功能演示",
        artwork: "city",
        episodes: 2,
    },
    DemoDefinition {
        id: "player-demo",
        title: "播放器实验室",
        description: "演示条目：所有剧集均指向已在 M3-M5 验证的 SampleLib 20 秒 MP4。",
        genre: "播放器测试",
        badge: "20 秒测试资源",
        artwork: "studio",
        episodes: 4,
    },
];

impl DemoDefinition {
    fn summary(&self) -> Series {
        Series {
            id: self.id.into(),
            title: self.title.into(),
            description: self.description.into(),
            genre: self.genre.into(),
            badge: self.badge.into(),
            artwork: self.artwork.into(),
            episode_count: self.episodes,
        }
    }

    fn episode(&self, index: usize) -> Episode {
        Episode {
            id: format!("ep-{:02}", index + 1),
            number: index + 1,
            title: format!("第 {} 集 · 测试片段", index + 1),
            runtime_label: "00:20".into(),
        }
    }
}

impl ContentProvider for LocalDemoProvider {
    fn search(&self, query: &str) -> Vec<Series> {
        let term = query.trim().to_lowercase();
        DEMOS
            .iter()
            .filter(|item| {
                term.is_empty()
                    || item.title.to_lowercase().contains(&term)
                    || item.description.to_lowercase().contains(&term)
                    || item.genre.to_lowercase().contains(&term)
            })
            .map(DemoDefinition::summary)
            .collect()
    }

    fn detail(&self, series_id: &str) -> Option<SeriesDetail> {
        DEMOS.iter().find(|item| item.id == series_id).map(|item| {
            SeriesDetail {
                series: item.summary(),
                episodes: (0..item.episodes).map(|i| item.episode(i)).collect(),
            }
        })
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<String, String> {
        let details = self.detail(series_id).ok_or("Unknown demo series")?;
        if !details.episodes.iter().any(|episode| episode.id == episode_id) {
            return Err("Unknown demo episode".into());
        }
        // Never accept caller-supplied media URLs through the catalog API.
        crate::player::source::public_https_url(PREVIEW_MP4)
    }
}

pub struct CatalogService {
    provider: Box<dyn ContentProvider>,
}

impl Default for CatalogService {
    fn default() -> Self {
        Self {
            provider: Box::new(LocalDemoProvider),
        }
    }
}

impl CatalogService {
    pub fn search(&self, query: Option<&str>) -> Result<Vec<Series>, String> {
        let query = query.unwrap_or("").trim();
        if query.len() > 128 {
            return Err("Search text is too long".into());
        }
        Ok(self.provider.search(query))
    }

    pub fn detail(&self, id: &str) -> Result<SeriesDetail, String> {
        self.provider.detail(id).ok_or("Series not found".into())
    }

    pub fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<String, String> {
        self.provider.resolve_episode(series_id, episode_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{CatalogService, ContentProvider, LocalDemoProvider, PREVIEW_MP4};

    #[test]
    fn catalogue_supports_search_and_stable_details() {
        let provider = LocalDemoProvider;
        assert_eq!(provider.search("").len(), 3);
        assert_eq!(provider.search("城市").len(), 1);
        assert_eq!(provider.search("完全不存在").len(), 0);
        let details = provider.detail("forest-demo").expect("forest demo");
        assert_eq!(details.series.episode_count, details.episodes.len());
        assert_eq!(details.episodes[0].id, "ep-01");
    }

    #[test]
    fn only_known_demo_ids_can_resolve_to_fixed_https_fixture() {
        let provider = LocalDemoProvider;
        assert_eq!(provider.resolve_episode("forest-demo", "ep-01").unwrap(), PREVIEW_MP4);
        assert!(provider.resolve_episode("forest-demo", "ep-99").is_err());
        assert!(provider.resolve_episode("missing", "ep-01").is_err());
        assert!(provider.resolve_episode("forest-demo", "https://evil.example/").is_err());
    }

    #[test]
    fn service_guards_unbounded_search_text() {
        let catalog = CatalogService::default();
        assert!(catalog.search(Some(&"x".repeat(129))).is_err());
        assert!(catalog.search(None).unwrap().len() == 3);
    }
}
