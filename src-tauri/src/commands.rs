use crate::catalog::{
    CatalogService, MediaTarget, ProviderInfo, SearchPage, Series, SeriesDetail,
};
use crate::local_library::{LocalLibrary, LocalMedia};
use crate::player::{
    controls::PlayerControl, service::PlayerService, state::PlayerSnapshot, window,
};
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
) -> Result<SearchPage, String> {
    catalog.search_page(
        query.as_deref(),
        source_id.as_deref(),
        offset.unwrap_or(0),
        limit.unwrap_or(24),
    )
}

#[tauri::command]
pub fn catalog_detail(
    catalog: State<'_, CatalogService>,
    series_id: String,
) -> Result<SeriesDetail, String> {
    catalog.detail(&series_id)
}

#[tauri::command]
pub fn catalog_play_episode(
    catalog: State<'_, CatalogService>,
    player: State<'_, PlayerService>,
    series_id: String,
    episode_id: String,
) -> Result<u64, String> {
    match catalog.resolve_episode(&series_id, &episode_id)? {
        MediaTarget::Https(url) => player.open_https(url),
        MediaTarget::LocalLibrary(id) => player.open_library(id),
    }
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
