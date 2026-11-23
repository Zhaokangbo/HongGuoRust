//! Provider registry: namespaced IDs, capability discovery and bounded search.
//! This layer NEVER opens media; it returns validated targets to PlayerService.
use super::hgcommunity::HgCommunityProvider;
use super::hongguo::HongGuoUnavailableProvider;
use super::local::LocalLibraryProvider;
use super::manifest::{OwnedManifestProvider, OwnedManifestStore};
use super::request::{RequestContext, RequestRegistry};
use super::{ContentProvider, LocalDemoProvider, MediaTarget, Series, SeriesDetail};
use crate::local_library::LocalLibrary;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

const MAX_QUERY_BYTES: usize = 128;
const MAX_ID_BYTES: usize = 128;
const MAX_OFFSET: usize = 10_000;
const MAX_PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub supports_search: bool,
    pub supports_detail: bool,
    pub supports_play: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub items: Vec<Series>,
    pub total: usize,
    pub next_offset: Option<usize>,
}

pub struct CatalogService {
    providers: Vec<Box<dyn ContentProvider>>,
    requests: RequestRegistry,
}

fn valid_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn scope_series(provider: &dyn ContentProvider, mut series: Series) -> Result<Series, String> {
    let id = series.id.as_str();
    if id.is_empty()
        || id.len() > MAX_ID_BYTES
        || id.contains(':')
        || id.chars().any(char::is_control)
    {
        return Err("invalid_provider_item_id: Provider returned an invalid item ID".into());
    }
    series.id = format!("{}:{}", provider.id(), series.id);
    series.provider_id = provider.id().into();
    Ok(series)
}

fn split_series_id(value: &str) -> Result<(&str, &str), String> {
    if value.is_empty() || value.len() > MAX_ID_BYTES + 33 || value.chars().any(char::is_control) {
        return Err("invalid_series_id: Invalid series ID".into());
    }
    // M6 callers using unscoped demo IDs continue to work.
    let (source, local_id) = value.split_once(':').unwrap_or(("demo", value));
    if !valid_provider_id(source)
        || local_id.is_empty()
        || local_id.len() > MAX_ID_BYTES
        || local_id.contains(':')
    {
        return Err("invalid_series_id: Invalid provider-scoped series ID".into());
    }
    Ok((source, local_id))
}

impl Default for CatalogService {
    fn default() -> Self {
        Self::from_providers(vec![Box::new(LocalDemoProvider)])
            .expect("built-in demo provider must have a valid unique ID")
    }
}

impl CatalogService {
    pub fn new(library: Arc<LocalLibrary>, manifest: Arc<OwnedManifestStore>) -> Self {
        let mut providers: Vec<Box<dyn ContentProvider>> = vec![
            Box::new(LocalDemoProvider),
            Box::new(LocalLibraryProvider::new(library)),
            Box::new(OwnedManifestProvider::new(manifest)),
            Box::new(HongGuoUnavailableProvider),
        ];
        // The unofficial community SDK is opt-in and debug-build only. Its
        // failure must never replace or unlock the real HongGuo placeholder.
        if let Some(experimental) = HgCommunityProvider::from_env() {
            providers.push(Box::new(experimental));
        }
        Self::from_providers(providers).expect("built-in provider IDs must be unique and valid")
    }

    /// Runtime registry for the desktop product. Real community content is
    /// the default source; demo and authorization placeholders stay out of the
    /// user-facing registry.
    pub fn new_for_desktop(
        manifest: Arc<OwnedManifestStore>,
        community_token: Option<String>,
    ) -> Self {
        Self::from_providers(vec![
            Box::new(OwnedManifestProvider::new(manifest)),
            Box::new(HgCommunityProvider::managed(community_token)),
        ])
        .expect("desktop provider IDs must be unique and valid")
    }

    fn from_providers(providers: Vec<Box<dyn ContentProvider>>) -> Result<Self, String> {
        let mut keys = HashSet::new();
        for provider in &providers {
            if !valid_provider_id(provider.id()) || !keys.insert(provider.id()) {
                return Err("invalid_provider_registry: Duplicate or invalid provider ID".into());
            }
        }
        Ok(Self {
            providers,
            requests: RequestRegistry::default(),
        })
    }

    pub fn sources(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| ProviderInfo {
                id: provider.id().into(),
                name: provider.name().into(),
                supports_search: provider.supports_search(),
                supports_detail: provider.supports_detail(),
                supports_play: provider.supports_play(),
            })
            .collect()
    }

    fn provider(&self, source: &str) -> Result<&dyn ContentProvider, String> {
        self.providers
            .iter()
            .find(|provider| provider.id() == source)
            .map(|provider| provider.as_ref())
            .ok_or_else(|| format!("provider_not_found: Unknown provider {source}"))
    }

    pub fn cancel_request(&self, id: u64) -> bool {
        self.requests.cancel(id)
    }

    /// A fresh request token never alters legacy IPC behavior.
    pub fn search_page_with_request(
        &self,
        query: Option<&str>,
        source: Option<&str>,
        offset: usize,
        limit: usize,
        request_id: u64,
        timeout_ms: u64,
    ) -> Result<SearchPage, String> {
        let guard = self.requests.begin(request_id, timeout_ms)?;
        self.search_page_inner(query, source, offset, limit, Some(guard.context()))
    }

    pub fn detail_with_request(
        &self,
        id: &str,
        request_id: u64,
        timeout_ms: u64,
    ) -> Result<SeriesDetail, String> {
        let guard = self.requests.begin(request_id, timeout_ms)?;
        self.detail_inner(id, Some(guard.context()))
    }

    /// Legacy IPC follows the same default source as the desktop registry.
    /// Test registries without a community provider continue to default to demo.
    pub fn search(&self, query: Option<&str>) -> Result<Vec<Series>, String> {
        Ok(self.search_page(query, None, 0, MAX_PAGE_SIZE)?.items)
    }

    /// Search is deterministic by provider registration order and provider item order.
    /// Offset/limit always apply to the combined results *after* namespace scoping.
    pub fn search_page(
        &self,
        query: Option<&str>,
        source: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<SearchPage, String> {
        self.search_page_inner(query, source, offset, limit, None)
    }

    fn search_page_inner(
        &self,
        query: Option<&str>,
        source: Option<&str>,
        offset: usize,
        limit: usize,
        request: Option<&RequestContext>,
    ) -> Result<SearchPage, String> {
        if let Some(request) = request {
            request.check()?;
        }
        let query = query.unwrap_or("").trim();
        if query.len() > MAX_QUERY_BYTES {
            return Err("invalid_query: Search text exceeds 128 bytes".into());
        }
        if limit == 0 || limit > MAX_PAGE_SIZE || offset > MAX_OFFSET {
            return Err("invalid_page: Unsupported search page range".into());
        }
        let source = source.unwrap_or(if self.providers.iter().any(|provider| provider.id() == "hgcommunity") {
            "hgcommunity"
        } else {
            "demo"
        });
        if source != "all" {
            self.provider(source)?;
        }
        let mut all = Vec::new();
        for provider in &self.providers {
            if source != "all" && provider.id() != source {
                continue;
            }
            let matches = match request {
                Some(ctx) => provider.search_with_request(query, ctx)?,
                None => provider.search(query)?,
            };
            for item in matches {
                if let Some(ctx) = request {
                    ctx.check()?;
                }
                all.push(scope_series(provider.as_ref(), item)?);
            }
        }
        if let Some(ctx) = request {
            ctx.check()?;
        }
        let total = all.len();
        let end = offset.saturating_add(limit).min(total);
        let items = if offset >= total {
            Vec::new()
        } else {
            all.into_iter().skip(offset).take(limit).collect()
        };
        let next_offset = if end < total { Some(end) } else { None };
        Ok(SearchPage {
            items,
            total,
            next_offset,
        })
    }

    pub fn detail(&self, id: &str) -> Result<SeriesDetail, String> {
        self.detail_inner(id, None)
    }

    fn detail_inner(
        &self,
        id: &str,
        request: Option<&RequestContext>,
    ) -> Result<SeriesDetail, String> {
        if let Some(ctx) = request {
            ctx.check()?;
        }
        let (source, local_id) = split_series_id(id)?;
        let provider = self.provider(source)?;
        let result = match request {
            Some(ctx) => provider.detail_with_request(local_id, ctx)?,
            None => provider.detail(local_id)?,
        };
        let mut details = result.ok_or_else(|| "series_not_found: Series not found".to_string())?;
        if let Some(ctx) = request {
            ctx.check()?;
        }
        details.series = scope_series(provider, details.series)?;
        Ok(details)
    }

    pub fn resolve_episode(
        &self,
        series_id: &str,
        episode_id: &str,
    ) -> Result<MediaTarget, String> {
        let (source, local_id) = split_series_id(series_id)?;
        if episode_id.is_empty()
            || episode_id.len() > MAX_ID_BYTES
            || episode_id.chars().any(char::is_control)
        {
            return Err("invalid_episode_id: Invalid episode ID".into());
        }
        let provider = self.provider(source)?;
        // UI capability flags are informational: native resolution must also
        // block any source without verified playback rights.
        if !provider.supports_play() {
            return Err("provider_not_authorized: 当前内容源没有播放授权".into());
        }
        let target = provider.resolve_episode(local_id, episode_id)?;
        match target {
            MediaTarget::Https(url) => {
                crate::player::source::public_https_url(&url).map(MediaTarget::Https)
            }
            MediaTarget::LocalLibrary(id) => {
                // Prevent a future third-party provider from spoofing a local
                // library ID. Native playback validates the same ID again.
                if source != "local" || local_id.parse::<u64>().ok() != Some(id) {
                    return Err("invalid_media_target: Local media does not match provider".into());
                }
                Ok(MediaTarget::LocalLibrary(id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Episode;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct CollisionProvider;

    impl ContentProvider for CollisionProvider {
        fn id(&self) -> &'static str {
            "other"
        }

        fn name(&self) -> &'static str {
            "Other demo"
        }

        fn search(&self, _query: &str) -> Result<Vec<Series>, String> {
            Ok(vec![Series {
                id: "forest-demo".into(),
                title: "Other".into(),
                description: String::new(),
                genre: String::new(),
                badge: String::new(),
                artwork: "studio".into(),
                cover_url: None,
                episode_count: 1,
                provider_id: "spoofed".into(),
            }])
        }

        fn detail(&self, _series_id: &str) -> Result<Option<SeriesDetail>, String> {
            Ok(Some(SeriesDetail {
                series: self.search("")?.remove(0),
                episodes: vec![Episode {
                    id: "ep-01".into(),
                    number: 1,
                    title: "Other".into(),
                    runtime_label: "00:20".into(),
                    playable: true,
                }],
            }))
        }

        fn resolve_episode(
            &self,
            _series_id: &str,
            _episode_id: &str,
        ) -> Result<MediaTarget, String> {
            Ok(MediaTarget::Https("https://example.org/clip.mp4".into()))
        }
    }

    #[test]
    fn desktop_registry_uses_real_community_source_without_demo_or_placeholder_data() {
        let catalog = CatalogService::new_for_desktop(
            Arc::new(OwnedManifestStore::new()),
            None,
        );
        let sources = catalog.sources();
        let ids: HashSet<_> = sources.iter().map(|source| source.id.as_str()).collect();
        assert!(ids.contains("hgcommunity"));
        assert!(ids.contains("manifest"));
        assert!(!ids.contains("demo"));
        assert!(!ids.contains("local"));
        assert!(!ids.contains("hongguo"));

        let error = catalog.search_page(None, None, 0, 24).unwrap_err();
        assert!(error.contains("hgcommunity_sidecar_unavailable"));
    }

    #[test]
    fn provider_id_namespace_prevents_collisions_and_legacy_demo_ids_work() {
        let service = CatalogService::from_providers(vec![
            Box::new(LocalDemoProvider),
            Box::new(CollisionProvider),
        ])
        .unwrap();
        let all = service.search_page(None, Some("all"), 0, 50).unwrap();
        assert_eq!(all.items.len(), 4);
        assert_eq!(all.items[0].id, "demo:forest-demo");
        assert_eq!(all.items[3].id, "other:forest-demo");
        assert_eq!(all.items[3].provider_id, "other");
        assert_eq!(
            service.detail("forest-demo").unwrap().series.id,
            "demo:forest-demo"
        );
        assert_eq!(
            service.detail("other:forest-demo").unwrap().series.id,
            "other:forest-demo"
        );
        assert!(service.detail("missing:forest-demo").is_err());
        assert!(service.detail("demo::bad").is_err());
        assert!(CatalogService::from_providers(vec![
            Box::new(LocalDemoProvider),
            Box::new(LocalDemoProvider),
        ])
        .is_err());
    }

    #[test]
    fn search_caps_offset_source_filter_and_page_boundaries() {
        let service = CatalogService::default();
        let first = service.search_page(None, Some("demo"), 0, 2).unwrap();
        assert_eq!(first.total, 3);
        assert_eq!(first.next_offset, Some(2));
        assert_eq!(first.items.len(), 2);
        let second = service.search_page(None, Some("demo"), 2, 2).unwrap();
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.next_offset, None);
        assert!(service.search_page(None, Some("unknown"), 0, 10).is_err());
        assert!(service.search_page(None, None, 0, 0).is_err());
        assert!(service.search_page(None, None, 0, 51).is_err());
        assert!(service
            .search_page(Some(&"x".repeat(129)), None, 0, 20)
            .is_err());
        assert_eq!(service.sources().len(), 1);
    }

    #[test]
    fn local_provider_uses_existing_imported_media_and_native_history_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hgr-m9-{}-{unique}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let media = dir.join("自己的视频.MP4");
        fs::write(&media, b"test").unwrap();
        let library = Arc::new(LocalLibrary::from_path(dir.join("library.json")));
        let added = library.add(media.clone()).unwrap();
        let manifest = Arc::new(OwnedManifestStore::from_test_path(
            dir.join("owned-manifest.json"),
        ));
        let service = CatalogService::new(Arc::clone(&library), manifest);

        let local = service
            .search_page(Some("自己的"), Some("local"), 0, 10)
            .unwrap();
        assert_eq!(local.total, 1);
        assert_eq!(local.items[0].id, format!("local:{}", added.id));
        assert_eq!(local.items[0].provider_id, "local");
        let detail = service.detail(&local.items[0].id).unwrap();
        assert_eq!(detail.episodes[0].id, "ep-01");
        assert_eq!(
            service
                .resolve_episode(&local.items[0].id, "ep-01")
                .unwrap(),
            MediaTarget::LocalLibrary(added.id)
        );
        assert!(service
            .resolve_episode(&local.items[0].id, "ep-99")
            .is_err());
        let mixed = service.search_page(None, Some("all"), 0, 50).unwrap();
        assert_eq!(mixed.total, 4);

        let hongguo = service
            .sources()
            .into_iter()
            .find(|source| source.id == "hongguo")
            .expect("gated HongGuo source must be registered");
        assert!(!hongguo.supports_search);
        assert!(!hongguo.supports_detail);
        assert!(!hongguo.supports_play);
        assert!(hongguo.name.contains("待授权"));
        assert_eq!(
            service
                .search_page(None, Some("hongguo"), 0, 24)
                .unwrap()
                .total,
            0
        );
        assert!(service.detail("hongguo:synthetic-1").is_err());
        assert!(service
            .resolve_episode("hongguo:synthetic-1", "ep-1")
            .is_err());
        drop(service);
        drop(library);
        fs::remove_dir_all(dir).unwrap();
    }

    struct SlowProvider;

    impl ContentProvider for SlowProvider {
        fn id(&self) -> &'static str {
            "slow"
        }

        fn name(&self) -> &'static str {
            "Slow test source"
        }

        fn search(&self, _query: &str) -> Result<Vec<Series>, String> {
            Ok(Vec::new())
        }

        fn detail(&self, _id: &str) -> Result<Option<SeriesDetail>, String> {
            Ok(None)
        }

        fn resolve_episode(&self, _id: &str, _episode_id: &str) -> Result<MediaTarget, String> {
            Err("not implemented".into())
        }

        fn search_with_request(
            &self,
            _query: &str,
            request: &RequestContext,
        ) -> Result<Vec<Series>, String> {
            for _ in 0..100 {
                request.check()?;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(Vec::new())
        }
    }

    #[test]
    fn slow_provider_respects_deadline_and_request_cleanup() {
        let catalog = CatalogService::from_providers(vec![Box::new(SlowProvider)]).unwrap();
        let err = catalog
            .search_page_with_request(None, Some("slow"), 0, 24, 77, 50)
            .unwrap_err();
        assert!(err.starts_with("provider_timeout:"));
        assert!(!catalog.cancel_request(77));
        assert!(catalog
            .search_page_with_request(None, Some("slow"), 0, 24, 77, 50)
            .is_err());
    }

    #[test]
    fn cancelling_one_provider_request_does_not_poison_the_registry() {
        let catalog =
            Arc::new(CatalogService::from_providers(vec![Box::new(SlowProvider)]).unwrap());
        let worker_catalog = Arc::clone(&catalog);
        let handle = std::thread::spawn(move || {
            worker_catalog.search_page_with_request(None, Some("slow"), 0, 24, 91, 2_000)
        });
        let mut cancelled = false;
        for _ in 0..100 {
            if catalog.cancel_request(91) {
                cancelled = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(cancelled, "request must register before cancellation");
        let err = handle.join().unwrap().unwrap_err();
        assert!(err.starts_with("provider_cancelled:"));
        assert!(!catalog.cancel_request(91));
        assert_eq!(
            catalog
                .search_page(None, Some("slow"), 0, 24)
                .unwrap()
                .total,
            0
        );
    }
}
