use crate::models::{RowResult, ScrapeOptions, SelfCheckResult, SessionStatus};
use crate::region::AmazonSession;
use crate::scraper::{self_check, ScrapeEngine};
use crate::sku;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, State};

pub struct AppState {
    pub session: Mutex<Option<AmazonSession>>,
    pub cancel_flag: Arc<AtomicBool>,
    pub last_rows: Mutex<Vec<RowResult>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            last_rows: Mutex::new(Vec::new()),
        }
    }
}

#[tauri::command]
pub async fn init_session(
    state: State<'_, AppState>,
    zip_code: Option<String>,
) -> Result<SessionStatus, String> {
    let zip = zip_code.unwrap_or_else(|| crate::config::DEFAULT_ZIP.to_string());
    let mut session = AmazonSession::new(&zip).map_err(|e| e.to_string())?;
    session.init().await.map_err(|e| e.to_string())?;
    let delivery = session.delivery_location.clone();
    *state.session.lock() = Some(session);
    Ok(SessionStatus {
        initialized: true,
        zip_code: zip,
        delivery_location: delivery.clone(),
        message: delivery
            .map(|d| format!("会话已初始化，配送地：{d}"))
            .unwrap_or_else(|| "会话已初始化".to_string()),
    })
}

#[tauri::command]
pub fn parse_skus(text: String) -> Result<(Vec<RowResult>, usize), String> {
    Ok(sku::parse_skus_from_text(&text))
}

#[tauri::command]
pub fn parse_skus_file(path: String) -> Result<(Vec<RowResult>, usize), String> {
    sku::parse_skus_from_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_scrape(
    app: AppHandle,
    state: State<'_, AppState>,
    rows: Vec<RowResult>,
    options: Option<ScrapeOptions>,
) -> Result<Vec<RowResult>, String> {
    let opts = options.unwrap_or_default();
    state.cancel_flag.store(false, Ordering::SeqCst);

    let mut session = {
        let guard = state.session.lock();
        if let Some(existing) = guard.as_ref() {
            AmazonSession::new(&existing.zip_code).map_err(|e| e.to_string())?
        } else {
            AmazonSession::new(&opts.zip_code).map_err(|e| e.to_string())?
        }
    };

    let results = ScrapeEngine::scrape_rows(
        &mut session,
        rows,
        opts.rate_per_sec,
        opts.concurrency,
        Arc::clone(&state.cancel_flag),
        app,
    )
    .await
    .map_err(|e| e.to_string())?;

    *state.session.lock() = Some(session);
    *state.last_rows.lock() = results.clone();
    Ok(results)
}

#[tauri::command]
pub async fn refresh_one(
    app: AppHandle,
    state: State<'_, AppState>,
    row: RowResult,
    options: Option<ScrapeOptions>,
) -> Result<RowResult, String> {
    let opts = options.unwrap_or_default();
    state.cancel_flag.store(false, Ordering::SeqCst);

    let mut session = {
        let guard = state.session.lock();
        if let Some(existing) = guard.as_ref() {
            AmazonSession::new(&existing.zip_code).map_err(|e| e.to_string())?
        } else {
            AmazonSession::new(&opts.zip_code).map_err(|e| e.to_string())?
        }
    };

    let results = ScrapeEngine::scrape_rows(
        &mut session,
        vec![row],
        opts.rate_per_sec,
        1,
        Arc::clone(&state.cancel_flag),
        app,
    )
    .await
    .map_err(|e| e.to_string())?;

    *state.session.lock() = Some(session);
    results
        .into_iter()
        .next()
        .ok_or_else(|| "刷新失败".to_string())
}

#[tauri::command]
pub async fn refresh_all(
    app: AppHandle,
    state: State<'_, AppState>,
    options: Option<ScrapeOptions>,
) -> Result<Vec<RowResult>, String> {
    let rows = state.last_rows.lock().clone();
    if rows.is_empty() {
        return Err("没有可刷新的结果".to_string());
    }
    start_scrape(app, state, rows, options).await
}

#[tauri::command]
pub fn export_csv(rows: Vec<RowResult>) -> Result<String, String> {
    let mut out = String::from("\u{feff}");
    out.push_str("SKU,dpCode,ASIN,价格,数值(JPY),Amazon链接,状态,错误,抓取时间\n");
    for row in rows {
        out.push_str(&csv_escape(&row.sku));
        out.push(',');
        out.push_str(&csv_escape(&row.dp_code));
        out.push(',');
        out.push_str(&csv_escape(&row.asin));
        out.push(',');
        out.push_str(&csv_escape(row.price_text.as_deref().unwrap_or("-")));
        out.push(',');
        out.push_str(&row.price_value.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()));
        out.push(',');
        out.push_str(&csv_escape(&row.amazon_url));
        out.push(',');
        out.push_str(&csv_escape(row.status.as_str()));
        out.push(',');
        out.push_str(&csv_escape(row.error.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(row.fetched_at.as_deref().unwrap_or("")));
        out.push('\n');
    }
    Ok(out)
}

#[tauri::command]
pub fn cancel_scrape(state: State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn run_self_check(
    state: State<'_, AppState>,
    zip_code: Option<String>,
) -> Result<SelfCheckResult, String> {
    let zip = zip_code.unwrap_or_else(|| crate::config::DEFAULT_ZIP.to_string());
    let mut session = AmazonSession::new(&zip).map_err(|e| e.to_string())?;
    let (ok, price_text, message) = self_check(&mut session).await.map_err(|e| e.to_string())?;
    *state.session.lock() = Some(session);
    Ok(SelfCheckResult {
        ok,
        asin: crate::config::SELF_CHECK_ASIN.to_string(),
        price_text,
        message,
    })
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
