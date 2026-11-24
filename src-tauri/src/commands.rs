use crate::account::{
    qr_login::QrLoginStart, AccountService, AccountStatusSnapshot, CloudLibrarySnapshot,
    QrPollSnapshot,
};
use crate::catalog::progress::{ProviderEpisodeProgress, ProviderProgressStore};
use crate::catalog::request::DEFAULT_TIMEOUT_MS;
use crate::catalog::{
    CatalogService, DanmakuItem, Episode, FavoriteSeries, FavoritesStore, ManifestSubscriptionInfo,
    MediaTarget, OwnedManifestStore, ProviderInfo, QualityOption, SearchPage, Series, SeriesDetail,
    WebDavStatus, WebDavStore,
};
use crate::local_library::{LocalLibrary, LocalMedia};
use crate::player::{
    controls::PlayerControl,
    overlay::{self, PlayerUi},
    preferences::{PlayerPreferences, PreferencesStore},
    preview::PosterCache,
    service::PlayerService,
    source::MediaSource,
    state::PlayerSnapshot,
    window,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewWindow};

/// M12-A read-only in-memory diagnostics; never returns names, paths or URLs.
#[tauri::command]
pub fn get_runtime_health(
    player: State<'_, PlayerService>,
) -> crate::player::health::RuntimeHealth {
    player.runtime_health()
}

#[tauri::command]
pub fn get_snapshot(player: State<'_, PlayerService>) -> PlayerSnapshot {
    static FIRST: AtomicBool = AtomicBool::new(true);
    let snapshot = player.snapshot();
    if FIRST.swap(false, Ordering::Relaxed) {
        tracing::info!(revision=snapshot.revision, state=?snapshot.state, "first snapshot IPC");
    }
    snapshot
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupPlayback {
    session_id: u64,
    title: String,
}

/// Consume a video file path passed via Windows "Open with" / CLI arguments on launch.
#[tauri::command]
pub fn open_startup_media(
    startup: State<'_, crate::StartupMedia>,
    player: State<'_, PlayerService>,
) -> Result<Option<StartupPlayback>, String> {
    let path = startup
        .0
        .lock()
        .map_err(|_| "startup state poisoned")?
        .take();
    let Some(path) = path else {
        return Ok(None);
    };
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("本地视频")
        .to_owned();
    let session_id = player.open(path)?;
    Ok(Some(StartupPlayback { session_id, title }))
}

#[tauri::command]
pub async fn open_local(window: WebviewWindow) -> Result<Option<u64>, String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<window::Picker>()
            .choose_video(owner)?
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
pub fn get_fullscreen(app: AppHandle) -> Result<bool, String> {
    let main = app
        .get_webview_window(overlay::MAIN)
        .ok_or("Missing main window")?;
    main.is_fullscreen().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_fullscreen(
    app: AppHandle,
    host: State<'_, crate::VideoHost>,
    ui: State<'_, PlayerUi>,
    fullscreen: bool,
) -> Result<bool, String> {
    let main = app
        .get_webview_window(overlay::MAIN)
        .ok_or("Missing main window")?;
    main.set_fullscreen(fullscreen)
        .map_err(|error| error.to_string())?;
    if let Ok(hwnd) = main.hwnd() {
        window::layout(hwnd.0, host.0);
    }
    if ui.is_active() {
        overlay::sync(&main);
    }
    main.is_fullscreen().map_err(|error| error.to_string())
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

/// Playback preferences contain no media source URLs or file paths.
#[tauri::command]
pub fn get_player_preferences(
    preferences: State<'_, Arc<PreferencesStore>>,
) -> Result<PlayerPreferences, String> {
    preferences.snapshot()
}

/// Auto-next is for the existing local playlist only; never auto-advance
/// Provider episodes or change the native playback worker.
#[tauri::command]
pub fn set_player_auto_next(
    preferences: State<'_, Arc<PreferencesStore>>,
    value: bool,
) -> Result<PlayerPreferences, String> {
    preferences.set_auto_next(value)
}

#[tauri::command]
pub fn catalog_search(
    catalog: State<'_, Arc<CatalogService>>,
    query: Option<String>,
) -> Result<Vec<Series>, String> {
    catalog.search(query.as_deref())
}

#[tauri::command]
pub fn catalog_sources(catalog: State<'_, Arc<CatalogService>>) -> Vec<ProviderInfo> {
    catalog.sources()
}

#[tauri::command]
pub async fn catalog_search_page(
    catalog: State<'_, Arc<CatalogService>>,
    query: Option<String>,
    source_id: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    request_id: Option<u64>,
    timeout_ms: Option<u64>,
) -> Result<SearchPage, String> {
    let catalog = Arc::clone(catalog.inner());
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(24);
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn catalog_detail(
    catalog: State<'_, Arc<CatalogService>>,
    series_id: String,
    request_id: Option<u64>,
    timeout_ms: Option<u64>,
) -> Result<SeriesDetail, String> {
    let catalog = Arc::clone(catalog.inner());
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(request_id) = request_id {
            catalog.detail_with_request(
                &series_id,
                request_id,
                timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
            )
        } else {
            catalog.detail(&series_id)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn catalog_cancel_request(catalog: State<'_, Arc<CatalogService>>, request_id: u64) -> bool {
    catalog.cancel_request(request_id)
}

#[tauri::command]
pub async fn catalog_play_episode(
    window: WebviewWindow,
    series_id: String,
    episode_id: String,
    quality: Option<u32>,
    resume_sec: Option<f64>,
) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = window.state::<Arc<CatalogService>>();
        let player = window.state::<PlayerService>();
        let detail = catalog.detail(&series_id)?;
        let scoped_series_id = detail.series.id;
        let episode_count = detail.series.episode_count.max(detail.episodes.len());
        let episode_number = detail
            .episodes
            .iter()
            .find(|ep| ep.id == episode_id)
            .map(|ep| ep.number)
            .unwrap_or(1);
        match catalog.resolve_episode_with_quality(&series_id, &episode_id, quality)? {
            MediaTarget::Https(url) => player.open_catalog_https(url, scoped_series_id, episode_id),
            MediaTarget::LocalLibrary(id) => player.open_library(id),
            MediaTarget::WebDav { url, auth_header } => {
                player.open_catalog_webdav(url, auth_header, scoped_series_id, episode_id)
            }
            MediaTarget::LocalStream {
                url,
                token_header,
                stable_key,
            } => player.open_catalog_local_stream(
                url,
                token_header,
                stable_key,
                scoped_series_id,
                episode_id,
                episode_number,
                episode_count,
                resume_sec,
            ),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn catalog_episode_qualities(
    catalog: State<'_, Arc<CatalogService>>,
    series_id: String,
    episode_id: String,
) -> Result<Vec<QualityOption>, String> {
    let catalog = Arc::clone(catalog.inner());
    tauri::async_runtime::spawn_blocking(move || catalog.episode_qualities(&series_id, &episode_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn catalog_episode_danmaku(
    catalog: State<'_, Arc<CatalogService>>,
    series_id: String,
    episode_id: String,
    duration_ms: Option<u64>,
) -> Result<Vec<DanmakuItem>, String> {
    let catalog = Arc::clone(catalog.inner());
    tauri::async_runtime::spawn_blocking(move || {
        catalog.episode_danmaku(&series_id, &episode_id, duration_ms)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Only return bookmarks whose provider target still resolves to the same media.
/// The frontend never receives the media source URL or its fingerprint.
#[tauri::command]
pub async fn catalog_episode_progress(
    catalog: State<'_, Arc<CatalogService>>,
    progress: State<'_, Arc<ProviderProgressStore>>,
    series_id: String,
) -> Result<Vec<ProviderEpisodeProgress>, String> {
    let catalog = Arc::clone(catalog.inner());
    let progress = Arc::clone(progress.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let detail = catalog.detail(&series_id)?;
        let mut result = Vec::new();
        for episode in &detail.episodes {
            if let Ok(Some(source_key)) =
                catalog.progress_source_key(&detail.series.id, &episode.id)
            {
                if let Some(record) = progress.get(&detail.series.id, &episode.id, &source_key)? {
                    result.push(record);
                }
            }
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
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
pub async fn catalog_recently_watched(
    catalog: State<'_, Arc<CatalogService>>,
    progress: State<'_, Arc<ProviderProgressStore>>,
    account: State<'_, Arc<AccountService>>,
    limit: Option<usize>,
) -> Result<Vec<RecentCatalogEpisode>, String> {
    let limit = limit.unwrap_or(12);
    if limit == 0 || limit > 24 {
        return Err("invalid_page: Recent history limit must be 1..24".into());
    }
    let catalog = Arc::clone(catalog.inner());
    let progress = Arc::clone(progress.inner());
    let account = Arc::clone(account.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let cloud_lib = account.cloud_library();
        let cloud_by_series: HashMap<String, _> = cloud_lib
            .history
            .iter()
            .cloned()
            .map(|entry| (entry.series.id.clone(), entry))
            .collect();

        let mut results = Vec::new();
        let mut seen = HashSet::new();
        // A capped scan protects IPC from very large history files.
        // The oldest records beyond this window are still retained on disk.
        for record in progress.list_recent()?.into_iter().take(120) {
            if seen.contains(&record.series_id) || record.series_id.starts_with("local:") {
                continue;
            }
            // Fast path: if this record matches a synced cloud history entry, reuse
            // its cached metadata without issuing per-item network detail requests.
            if let Some(cloud_entry) = cloud_by_series.get(&record.series_id) {
                if cloud_entry.episode_id == record.episode_id {
                    seen.insert(record.series_id.clone());
                    results.push(RecentCatalogEpisode {
                        series: cloud_entry.series.clone(),
                        episode: Episode {
                            id: cloud_entry.episode_id.clone(),
                            number: cloud_entry.episode_number,
                            title: cloud_entry.episode_title.clone(),
                            runtime_label: "—".into(),
                            playable: true,
                        },
                        progress: record,
                    });
                    if results.len() == limit {
                        break;
                    }
                    continue;
                }
            }
            let detail = match catalog.detail(&record.series_id) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let episode = match detail.episodes.iter().find(|ep| ep.id == record.episode_id) {
                Some(value) => value.clone(),
                None => continue,
            };
            let Some(source_key) = catalog
                .progress_source_key(&detail.series.id, &record.episode_id)
                .ok()
                .flatten()
            else {
                continue;
            };
            if progress
                .get(&detail.series.id, &record.episode_id, &source_key)?
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

        // Fill remaining slots with any synced cloud history items not yet in local store.
        if results.len() < limit {
            for entry in cloud_lib.history {
                if seen.contains(&entry.series.id) {
                    continue;
                }
                seen.insert(entry.series.id.clone());
                let completed = entry
                    .duration_sec
                    .is_some_and(|d| d > 5.0 && entry.position_sec >= d - 5.0);
                results.push(RecentCatalogEpisode {
                    series: entry.series.clone(),
                    episode: Episode {
                        id: entry.episode_id.clone(),
                        number: entry.episode_number,
                        title: entry.episode_title.clone(),
                        runtime_label: "—".into(),
                        playable: true,
                    },
                    progress: ProviderEpisodeProgress {
                        series_id: entry.series.id,
                        episode_id: entry.episode_id,
                        position_sec: entry.position_sec,
                        duration_sec: entry.duration_sec,
                        completed,
                        last_played_ms: entry.updated_at_ms,
                    },
                });
                if results.len() == limit {
                    break;
                }
            }
        }

        results.sort_by(|a, b| b.progress.last_played_ms.cmp(&a.progress.last_played_ms));
        results.truncate(limit);
        Ok(results)
    })
    .await
    .map_err(|e| e.to_string())?
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
pub async fn catalog_subscribe_manifest(
    manifest: State<'_, Arc<OwnedManifestStore>>,
    url: String,
) -> Result<usize, String> {
    let manifest = Arc::clone(manifest.inner());
    tauri::async_runtime::spawn_blocking(move || manifest.import_remote_url(&url))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn catalog_refresh_manifest(
    manifest: State<'_, Arc<OwnedManifestStore>>,
) -> Result<usize, String> {
    let manifest = Arc::clone(manifest.inner());
    tauri::async_runtime::spawn_blocking(move || manifest.refresh_subscription())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn catalog_manifest_info(
    manifest: State<'_, Arc<OwnedManifestStore>>,
) -> ManifestSubscriptionInfo {
    manifest.subscription_info()
}

#[tauri::command]
pub fn webdav_get_status(webdav: State<'_, Arc<WebDavStore>>) -> WebDavStatus {
    webdav.status()
}

#[tauri::command]
pub async fn webdav_configure(
    webdav: State<'_, Arc<WebDavStore>>,
    name: String,
    base_url: String,
    username: String,
    password: String,
) -> Result<WebDavStatus, String> {
    let webdav = Arc::clone(webdav.inner());
    tauri::async_runtime::spawn_blocking(move || {
        webdav.configure_and_sync(name, base_url, username, password)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn webdav_sync(webdav: State<'_, Arc<WebDavStore>>) -> Result<WebDavStatus, String> {
    let webdav = Arc::clone(webdav.inner());
    tauri::async_runtime::spawn_blocking(move || webdav.sync())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn webdav_clear(webdav: State<'_, Arc<WebDavStore>>) -> Result<(), String> {
    webdav.clear()
}

fn merge_favorites(local: Vec<FavoriteSeries>, account: &AccountService) -> Vec<FavoriteSeries> {
    let cloud = account.cloud_library().favorites;
    if cloud.is_empty() {
        return local;
    }
    let mut by_id: HashMap<String, FavoriteSeries> = HashMap::new();
    for item in local {
        by_id.insert(item.id.clone(), item);
    }
    for cf in cloud {
        let id = cf.series.id.clone();
        match by_id.get(&id) {
            Some(existing) if existing.added_at_ms > cf.added_at_ms => {}
            _ => {
                by_id.insert(
                    id,
                    FavoriteSeries {
                        id: cf.series.id,
                        title: cf.series.title,
                        description: cf.series.description,
                        genre: cf.series.genre,
                        badge: cf.series.badge,
                        artwork: cf.series.artwork,
                        cover_url: cf.series.cover_url,
                        episode_count: cf.series.episode_count,
                        provider_id: cf.series.provider_id,
                        added_at_ms: cf.added_at_ms,
                    },
                );
            }
        }
    }
    let mut merged: Vec<FavoriteSeries> = by_id.into_values().collect();
    merged.sort_by(|a, b| {
        b.added_at_ms
            .cmp(&a.added_at_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    merged
}

#[tauri::command]
pub fn catalog_favorites_list(
    favorites: State<'_, Arc<FavoritesStore>>,
    account: State<'_, Arc<AccountService>>,
) -> Result<Vec<FavoriteSeries>, String> {
    let local = favorites.list()?;
    Ok(merge_favorites(local, account.inner()))
}

#[tauri::command]
pub async fn catalog_favorite_set(
    catalog: State<'_, Arc<CatalogService>>,
    favorites: State<'_, Arc<FavoritesStore>>,
    account: State<'_, Arc<AccountService>>,
    series_id: String,
    favorite: bool,
) -> Result<Vec<FavoriteSeries>, String> {
    let catalog = Arc::clone(catalog.inner());
    let favorites = Arc::clone(favorites.inner());
    let account = Arc::clone(account.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let (scoped_id, local) = if favorite {
            let detail = catalog.detail(&series_id)?;
            let id = detail.series.id.clone();
            let list = favorites.add(detail.series)?;
            (id, list)
        } else {
            let list = favorites.remove(&series_id)?;
            (series_id.clone(), list)
        };
        if let Err(err) = account.set_cloud_favorite(&scoped_id, favorite) {
            tracing::warn!(error = %err, "cloud favorite write failed; local favorite preserved");
        }
        Ok(merge_favorites(local, &account))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn account_status(account: State<'_, Arc<AccountService>>) -> AccountStatusSnapshot {
    account.status()
}

#[tauri::command]
pub async fn account_qr_start(
    account: State<'_, Arc<AccountService>>,
) -> Result<QrLoginStart, String> {
    let account = Arc::clone(account.inner());
    tauri::async_runtime::spawn_blocking(move || account.start_qr())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn account_qr_poll(
    account: State<'_, Arc<AccountService>>,
    session_id: u64,
) -> Result<QrPollSnapshot, String> {
    let account = Arc::clone(account.inner());
    tauri::async_runtime::spawn_blocking(move || account.poll_qr(session_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn account_qr_cancel(account: State<'_, Arc<AccountService>>) {
    account.cancel_qr();
}

#[tauri::command]
pub async fn account_sync(
    account: State<'_, Arc<AccountService>>,
    progress: State<'_, Arc<ProviderProgressStore>>,
) -> Result<CloudLibrarySnapshot, String> {
    let account = Arc::clone(account.inner());
    let progress = Arc::clone(progress.inner());
    tauri::async_runtime::spawn_blocking(move || account.sync_now(&progress))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn account_cloud_library(account: State<'_, Arc<AccountService>>) -> CloudLibrarySnapshot {
    account.cloud_library()
}

#[tauri::command]
pub fn account_logout(
    account: State<'_, Arc<AccountService>>,
) -> Result<AccountStatusSnapshot, String> {
    account.logout()
}

#[tauri::command]
pub fn set_catalog_visible(host: State<'_, crate::VideoHost>, visible: bool) {
    window::set_video_visible(host.0, !visible);
}

#[tauri::command]
pub fn player_present(
    app: AppHandle,
    host: State<'_, crate::VideoHost>,
    ui: State<'_, PlayerUi>,
    context: serde_json::Value,
) -> Result<(), String> {
    overlay::present(&app, host.0, &ui, context)
}

#[tauri::command]
pub fn player_update_context(
    app: AppHandle,
    ui: State<'_, PlayerUi>,
    context: serde_json::Value,
) -> Result<(), String> {
    overlay::update(&app, &ui, context)
}

#[tauri::command]
pub fn player_dismiss(
    app: AppHandle,
    host: State<'_, crate::VideoHost>,
    ui: State<'_, PlayerUi>,
    player: State<'_, PlayerService>,
) -> Result<(), String> {
    let _ = player.stop();
    overlay::dismiss(&app, host.0, &ui)
}

#[tauri::command]
pub fn get_player_context(ui: State<'_, PlayerUi>) -> Option<serde_json::Value> {
    ui.context()
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

/// Extract (or load cached) cover frame for a local library video.
#[tauri::command]
pub async fn local_poster_frame(
    library: State<'_, Arc<LocalLibrary>>,
    posters: State<'_, Arc<PosterCache>>,
    id: u64,
) -> Result<Option<String>, String> {
    let path = match library.resolve_path(id) {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let posters = Arc::clone(posters.inner());
    tauri::async_runtime::spawn_blocking(move || posters.poster_frame(&MediaSource::Local(path)))
        .await
        .map_err(|e| e.to_string())
}

/// Extract (or load cached) cover frame from the first playable episode of a series.
#[tauri::command]
pub async fn catalog_poster_frame(
    catalog: State<'_, Arc<CatalogService>>,
    library: State<'_, Arc<LocalLibrary>>,
    posters: State<'_, Arc<PosterCache>>,
    series_id: String,
) -> Result<Option<String>, String> {
    if !catalog.supports_poster_extract(&series_id) {
        return Ok(None);
    }
    let catalog = Arc::clone(catalog.inner());
    let library = Arc::clone(library.inner());
    let posters = Arc::clone(posters.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let detail = match catalog.detail(&series_id) {
            Ok(detail) => detail,
            Err(_) => return Ok(None),
        };
        let Some(first_ep) = detail.episodes.iter().find(|ep| ep.playable) else {
            return Ok(None);
        };
        let source = match catalog.resolve_episode(&detail.series.id, &first_ep.id) {
            Ok(MediaTarget::LocalLibrary(id)) => match library.resolve_path(id) {
                Ok(path) => MediaSource::Local(path),
                Err(_) => return Ok(None),
            },
            Ok(MediaTarget::Https(url)) => MediaSource::Https(url),
            Ok(MediaTarget::WebDav { url, auth_header }) => {
                MediaSource::WebDav { url, auth_header }
            }
            Ok(MediaTarget::LocalStream { .. }) | Err(_) => return Ok(None),
        };
        Ok(posters.poster_frame(&source))
    })
    .await
    .map_err(|e| e.to_string())?
}
