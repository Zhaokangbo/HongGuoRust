use crate::catalog::progress::{ProviderEpisodeProgress, ProviderProgressStore};
use crate::catalog::request::DEFAULT_TIMEOUT_MS;
use crate::catalog::{
    CatalogService, MediaTarget, OwnedManifestStore, ProviderInfo, SearchPage, Series, SeriesDetail,
};
use crate::local_library::{LocalLibrary, LocalMedia};
use crate::player::{
    controls::PlayerControl, service::PlayerService, state::PlayerSnapshot, window,
};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Manager, State, WebviewWindow};

#[tauri::command]
pub fn get_snapshot(player: State<'_, PlayerService>) -> PlayerSnapshot {
    static FIRST: AtomicBool = AtomicBool::new(true);
    let snapshot = player.snapshot();
    if FIRST.swap(false, Ordering::Relaxed) {
        tracing::info!(revision=snapshot.revision, state=?snapshot.state, "first snapshot IPC");
    }
    snapshot
}

#[tauri::command]
pub async fn open_local(window: WebviewWindow) -> Result<Option<u64>, String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<window::Picker>()
            .choose(owner)?
            .map(|path| window.state::<PlayerService>().open(path))
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn stop(player: State<'_, PlayerService>) -> Result<(), String> {
    player.stop()
}

#[tauri::command]
pub fn open_https(player: State<'_, PlayerService>, url: String) -> Result<u64, String> {
    player.open_https(url)
}

/// M11-C1: metadata-only track list, bound to the current native session.
#[tauri::command]
pub async fn player_tracks(
    window: WebviewWindow,
    session_id: u64,
) -> Result<crate::player::tracks::TrackList, String> {
    tauri::async_runtime::spawn_blocking(move || window.state::<PlayerService>().tracks(session_id))
        .await
        .map_err(|error| error.to_string())?
}

/// Select an existing audio/subtitle track; None disables subtitles only.
#[tauri::command]
pub async fn player_select_track(
    window: WebviewWindow,
    session_id: u64,
    kind: String,
    track_id: Option<i64>,
) -> Result<crate::player::tracks::TrackList, String> {
    let kind = crate::player::tracks::TrackKind::from_ipc(&kind).map_err(str::to_owned)?;
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<PlayerService>()
            .select_track(session_id, kind, track_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// M11-B thumbnail IPC accepts only the current session and timestamp.
/// The Rust-owned media URL/local path remains private.
#[tauri::command]
pub async fn player_preview_frame(
    window: WebviewWindow,
    session_id: u64,
    seconds: f64,
) -> Result<Option<crate::player::preview::PreviewFrame>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<PlayerService>()
            .preview_frame(session_id, seconds)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_fullscreen(window: WebviewWindow) -> Result<bool, String> {
    window.is_fullscreen().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_fullscreen(window: WebviewWindow, fullscreen: bool) -> Result<bool, String> {
    window
        .set_fullscreen(fullscreen)
        .map_err(|error| error.to_string())?;
    window.is_fullscreen().map_err(|error| error.to_string())
}

async fn dispatch_control(window: WebviewWindow, request: PlayerControl) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || window.state::<PlayerService>().control(request))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_paused(window: WebviewWindow, paused: bool) -> Result<(), String> {
    dispatch_control(window, PlayerControl::Pause(paused)).await
}

#[tauri::command]
pub async fn seek_to(window: WebviewWindow, seconds: f64) -> Result<(), String> {
    dispatch_control(window, PlayerControl::SeekAbsolute(seconds)).await
}

#[tauri::command]
pub async fn set_rate(window: WebviewWindow, rate: f64) -> Result<(), String> {
    dispatch_control(window, PlayerControl::SetRate(rate)).await
}

#[tauri::command]
pub async fn set_volume(window: WebviewWindow, volume: f64) -> Result<(), String> {
    dispatch_control(window, PlayerControl::SetVolume(volume)).await
}

#[tauri::command]
pub fn catalog_search(
    catalog: State<'_, CatalogService>,
    query: Option<String>,
) -> Result<Vec<Series>, String> {
    catalog.search(query.as_deref())
}

#[tauri::command]
pub fn catalog_sources(catalog: State<'_, CatalogService>) -> Vec<ProviderInfo> {
    catalog.sources()
}

#[tauri::command]
pub fn catalog_search_page(
    catalog: State<'_, CatalogService>,
    query: Option<String>,
    source_id: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    request_id: Option<u64>,
    timeout_ms: Option<u64>,
) -> Result<SearchPage, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(24);
    if let Some(request_id) = request_id {
        catalog.search_page_with_request(
            query.as_deref(),
            source_id.as_deref(),
            offset,
            limit,
            request_id,
            timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
        )
    } else {
        catalog.search_page(query.as_deref(), source_id.as_deref(), offset, limit)
    }
}

#[tauri::command]
pub fn catalog_detail(
    catalog: State<'_, CatalogService>,
    series_id: String,
    request_id: Option<u64>,
    timeout_ms: Option<u64>,
) -> Result<SeriesDetail, String> {
    if let Some(request_id) = request_id {
        catalog.detail_with_request(
            &series_id,
            request_id,
            timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
        )
    } else {
        catalog.detail(&series_id)
    }
}

#[tauri::command]
pub fn catalog_cancel_request(catalog: State<'_, CatalogService>, request_id: u64) -> bool {
    catalog.cancel_request(request_id)
}

#[tauri::command]
pub fn catalog_play_episode(
    catalog: State<'_, CatalogService>,
    player: State<'_, PlayerService>,
    series_id: String,
    episode_id: String,
) -> Result<u64, String> {
    let scoped_series_id = catalog.detail(&series_id)?.series.id;
    match catalog.resolve_episode(&series_id, &episode_id)? {
        MediaTarget::Https(url) => player.open_catalog_https(url, scoped_series_id, episode_id),
        MediaTarget::LocalLibrary(id) => player.open_library(id),
    }
}

/// Only return bookmarks whose provider target still resolves to the same media.
/// The frontend never receives the media source URL or its fingerprint.
#[tauri::command]
pub fn catalog_episode_progress(
    catalog: State<'_, CatalogService>,
    progress: State<'_, Arc<ProviderProgressStore>>,
    series_id: String,
) -> Result<Vec<ProviderEpisodeProgress>, String> {
    let detail = catalog.detail(&series_id)?;
    let mut result = Vec::new();
    for episode in &detail.episodes {
        match catalog.resolve_episode(&detail.series.id, &episode.id) {
            Ok(MediaTarget::Https(url)) => {
                if let Some(record) = progress.get(&detail.series.id, &episode.id, &url)? {
                    result.push(record);
                }
            }
            Ok(MediaTarget::LocalLibrary(_)) => {}
            Err(_) => { /* missing or invalid source cannot return a stale bookmark */ }
        }
    }
    Ok(result)
}

/// Metadata-only recent series entries. Only media currently resolving to
/// the same source may appear. This neither exposes source URLs nor changes
/// the playback state; local items remain under M7 LocalLibrary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCatalogEpisode {
    series: Series,
    episode: crate::catalog::Episode,
    progress: ProviderEpisodeProgress,
}

#[tauri::command]
pub fn catalog_recently_watched(
    catalog: State<'_, CatalogService>,
    progress: State<'_, Arc<ProviderProgressStore>>,
    limit: Option<usize>,
) -> Result<Vec<RecentCatalogEpisode>, String> {
    let limit = limit.unwrap_or(12);
    if limit == 0 || limit > 24 {
        return Err("invalid_page: Recent history limit must be 1..24".into());
    }
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    // A capped scan protects synchronous IPC from very large history files.
    // The oldest records beyond this window are still retained on disk.
    for record in progress.list_recent()?.into_iter().take(120) {
        if seen.contains(&record.series_id) || record.series_id.starts_with("local:") {
            continue;
        }
        let detail = match catalog.detail(&record.series_id) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let episode = match detail.episodes.iter().find(|ep| ep.id == record.episode_id) {
            Some(value) => value.clone(),
            None => continue,
        };
        let url = match catalog.resolve_episode(&detail.series.id, &record.episode_id) {
            Ok(MediaTarget::Https(url)) => url,
            _ => continue,
        };
        if progress
            .get(&detail.series.id, &record.episode_id, &url)?
            .is_none()
        {
            continue;
        }
        seen.insert(record.series_id.clone());
        results.push(RecentCatalogEpisode {
            series: detail.series,
            episode,
            progress: record,
        });
        if results.len() == limit {
            break;
        }
    }
    Ok(results)
}

#[tauri::command]
pub async fn catalog_import_manifest(window: WebviewWindow) -> Result<Option<usize>, String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<window::Picker>()
            .choose_manifest(owner)?
            .map(|path| window.state::<Arc<OwnedManifestStore>>().import_file(&path))
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn catalog_clear_manifest(manifest: State<'_, Arc<OwnedManifestStore>>) -> Result<(), String> {
    manifest.clear()
}

#[tauri::command]
pub fn set_catalog_visible(host: State<'_, crate::VideoHost>, visible: bool) {
    window::set_video_visible(host.0, !visible);
}

#[tauri::command]
pub fn library_list(library: State<'_, Arc<LocalLibrary>>) -> Result<Vec<LocalMedia>, String> {
    library.list()
}

#[tauri::command]
pub async fn library_add_local(window: WebviewWindow) -> Result<Option<LocalMedia>, String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<window::Picker>()
            .choose(owner)?
            .map(|path| window.state::<Arc<LocalLibrary>>().add(path))
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn library_remove(library: State<'_, Arc<LocalLibrary>>, id: u64) -> Result<(), String> {
    library.remove(id)
}

#[tauri::command]
pub fn library_play(player: State<'_, PlayerService>, id: u64) -> Result<u64, String> {
    player.open_library(id)
}

#[tauri::command]
pub fn library_clear_history(library: State<'_, Arc<LocalLibrary>>, id: u64) -> Result<(), String> {
    library.clear_history(id)
}

#[tauri::command]
pub fn library_set_favorite(
    library: State<'_, Arc<LocalLibrary>>,
    id: u64,
    favorite: bool,
) -> Result<(), String> {
    library.set_favorite(id, favorite)
}
