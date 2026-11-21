//! Read-only adapter to existing user-imported MP4s. Never scans folders or
//! exposes absolute file paths to the catalog UI.
use super::{ContentProvider, Episode, MediaTarget, Series, SeriesDetail};
use crate::local_library::{LocalLibrary, LocalMedia};
use std::sync::Arc;

pub struct LocalLibraryProvider {
    library: Arc<LocalLibrary>,
}

impl LocalLibraryProvider {
    pub fn new(library: Arc<LocalLibrary>) -> Self {
        Self { library }
    }

    fn series(item: &LocalMedia) -> Series {
        Series {
            id: item.id.to_string(),
            title: item.title.clone(),
            description: "来自“我的视频”的本地 MP4。文件保留在原始位置，不复制、不上传。".into(),
            genre: "我的视频".into(),
            badge: if item.available {
                "本地媒体".into()
            } else {
                "文件不可用".into()
            },
            artwork: "studio".into(),
            episode_count: 1,
            provider_id: "local".into(),
        }
    }

    fn find(&self, id: &str) -> Result<Option<LocalMedia>, String> {
        let id = id.parse::<u64>().map_err(|_| "Invalid local media ID")?;
        Ok(self.library.list()?.into_iter().find(|item| item.id == id))
    }
}

impl ContentProvider for LocalLibraryProvider {
    fn id(&self) -> &'static str {
        "local"
    }

    fn name(&self) -> &'static str {
        "本地媒体"
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        let term = query.trim().to_lowercase();
        Ok(self
            .library
            .list()?
            .iter()
            .filter(|item| {
                term.is_empty()
                    || item.title.to_lowercase().contains(&term)
                    || item.filename.to_lowercase().contains(&term)
            })
            .map(Self::series)
            .collect())
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        Ok(self.find(series_id)?.map(|item| SeriesDetail {
            series: Self::series(&item),
            episodes: vec![Episode {
                id: "ep-01".into(),
                number: 1,
                title: "播放本地视频".into(),
                runtime_label: "MP4".into(),
            }],
        }))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        if episode_id != "ep-01" {
            return Err("Unknown local media episode".into());
        }
        let id = series_id
            .parse::<u64>()
            .map_err(|_| "Invalid local media ID")?;
        self.library.resolve_path(id)?;
        Ok(MediaTarget::LocalLibrary(id))
    }
}
