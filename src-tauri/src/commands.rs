use crate::models::{ParseSkusResult, ProxyConfig, RowResult, ScrapeOptions, SelfCheckResult, SessionStatus};
use crate::scraper::ProgressCallback;
use crate::service;
use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn init_session(
    state: State<'_, AppState>,
    zip_code: Option<String>,
) -> Result<SessionStatus, String> {
    service::init_session(&state, zip_code).await
}

#[tauri::command]
pub fn parse_skus(text: String) -> Result<ParseSkusResult, String> {
    Ok(service::parse_skus(&text))
}

#[tauri::command]
pub fn parse_skus_file(path: String) -> Result<ParseSkusResult, String> {
    crate::sku::parse_skus_from_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_scrape(
    app: AppHandle,
    state: State<'_, AppState>,
    rows: Vec<RowResult>,
    options: Option<ScrapeOptions>,
) -> Result<Vec<RowResult>, String> {
    let on_progress = Some(tauri_progress_callback(app));
    service::start_scrape(&state, rows, options, on_progress).await
}

#[tauri::command]
pub async fn refresh_one(
    app: AppHandle,
    state: State<'_, AppState>,
    row: RowResult,
    options: Option<ScrapeOptions>,
) -> Result<RowResult, String> {
    let on_progress = Some(tauri_progress_callback(app));
    service::refresh_one(&state, row, options, on_progress).await
}

#[tauri::command]
pub async fn refresh_all(
    app: AppHandle,
    state: State<'_, AppState>,
    options: Option<ScrapeOptions>,
) -> Result<Vec<RowResult>, String> {
    let on_progress = Some(tauri_progress_callback(app));
    service::refresh_all(&state, options, on_progress).await
}

#[tauri::command]
pub fn export_csv(rows: Vec<RowResult>) -> Result<String, String> {
    Ok(service::export_csv(&rows))
}

#[tauri::command]
pub fn cancel_scrape(state: State<'_, AppState>) {
    service::cancel_scrape(&state);
}

#[tauri::command]
pub async fn run_self_check(
    state: State<'_, AppState>,
    zip_code: Option<String>,
) -> Result<SelfCheckResult, String> {
    service::run_self_check(&state, zip_code).await
}

#[tauri::command]
pub fn get_proxy(state: State<'_, AppState>) -> Result<ProxyConfig, String> {
    Ok(service::get_proxy(&state))
}

#[tauri::command]
pub fn set_proxy(state: State<'_, AppState>, config: ProxyConfig) -> Result<ProxyConfig, String> {
    service::set_proxy(&state, config)
}

#[tauri::command]
pub async fn test_proxy(
    config: ProxyConfig,
    zip_code: Option<String>,
) -> Result<SelfCheckResult, String> {
    service::test_proxy(&config, zip_code).await
}

fn tauri_progress_callback(app: AppHandle) -> ProgressCallback {
    Arc::new(move |progress| {
        let _ = app.emit("scrape-progress", progress);
    })
}
