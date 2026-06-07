use crate::models::{ParseSkusResult, ProxyConfig, RowResult, ScrapeOptions, SelfCheckResult, SessionStatus};
use crate::proxy;
use crate::region::AmazonSession;
use crate::scraper::{self_check, ProgressCallback, ScrapeEngine};
use crate::state::AppState;
use crate::sku;
use std::sync::atomic::Ordering;

pub fn get_proxy(state: &AppState) -> ProxyConfig {
    state.proxy_config()
}

pub fn set_proxy(state: &AppState, config: ProxyConfig) -> Result<ProxyConfig, String> {
    if let Some(dir) = state.config_dir() {
        proxy::save_proxy_to_dir(&dir, &config)?;
    }
    state.set_proxy_config(config.clone());
    state.clear_session();
    Ok(config)
}

pub async fn test_proxy(config: &ProxyConfig, zip_code: Option<String>) -> Result<SelfCheckResult, String> {
    let zip = zip_code.unwrap_or_else(|| crate::config::DEFAULT_ZIP.to_string());
    let mut session = AmazonSession::new(&zip, config).map_err(|e| e.to_string())?;
    let (ok, price_text, currency, message) = self_check(&mut session)
        .await
        .map_err(|e| crate::config::friendly_network_error(e))?;
    Ok(SelfCheckResult {
        ok,
        asin: crate::config::SELF_CHECK_ASIN.to_string(),
        price_text,
        currency,
        message,
    })
}

pub async fn init_session(
    state: &AppState,
    zip_code: Option<String>,
) -> Result<SessionStatus, String> {
    let zip = zip_code.unwrap_or_else(|| crate::config::DEFAULT_ZIP.to_string());
    let proxy = state.proxy_config();

    if let Some(existing) = state.session.lock().clone() {
        if existing.delivery_location.is_some() {
            return Ok(SessionStatus {
                initialized: true,
                zip_code: existing.zip_code.clone(),
                delivery_location: existing.delivery_location.clone(),
                message: existing
                    .delivery_location
                    .clone()
                    .map(|d| format!("会话已就绪 · 日语 (JPY) · 配送地：{d}"))
                    .unwrap_or_else(|| "会话已就绪".to_string()),
            });
        }
    }

    let mut session = AmazonSession::new(&zip, &proxy).map_err(|e| e.to_string())?;
    session
        .init_with_retry()
        .await
        .map_err(|e| crate::config::friendly_network_error(e))?;
    let delivery = session.delivery_location.clone();
    *state.session.lock() = Some(session);
    Ok(SessionStatus {
        initialized: true,
        zip_code: zip,
        delivery_location: delivery.clone(),
        message: delivery
            .clone()
            .map(|d| format!("已设置日语 (JPY) 与日本配送：{d}"))
            .unwrap_or_else(|| "已设置日语 (JPY) 与日本配送".to_string()),
    })
}

pub fn parse_skus(text: &str) -> ParseSkusResult {
    sku::parse_skus_from_text(text)
}

pub async fn start_scrape(
    state: &AppState,
    rows: Vec<RowResult>,
    options: Option<ScrapeOptions>,
    on_progress: Option<ProgressCallback>,
) -> Result<Vec<RowResult>, String> {
    let opts = options.unwrap_or_default();
    let proxy = state.proxy_config();
    state.cancel_flag.store(false, Ordering::SeqCst);

    let mut session = {
        let guard = state.session.lock();
        if let Some(existing) = guard.as_ref() {
            existing.clone()
        } else {
            AmazonSession::new(&opts.zip_code, &proxy).map_err(|e| e.to_string())?
        }
    };

    let results = ScrapeEngine::scrape_rows(
        &mut session,
        rows,
        opts.request_interval_ms,
        opts.concurrency,
        std::sync::Arc::clone(&state.cancel_flag),
        on_progress,
        true,
    )
    .await
    .map_err(|e| crate::config::friendly_network_error(e))?;

    *state.session.lock() = Some(session);
    *state.last_rows.lock() = results.clone();
    Ok(results)
}

pub async fn refresh_one(
    state: &AppState,
    row: RowResult,
    options: Option<ScrapeOptions>,
    on_progress: Option<ProgressCallback>,
) -> Result<RowResult, String> {
    let opts = options.unwrap_or_default();
    let proxy = state.proxy_config();
    state.cancel_flag.store(false, Ordering::SeqCst);

    let mut session = {
        let guard = state.session.lock();
        if let Some(existing) = guard.as_ref() {
            existing.clone()
        } else {
            AmazonSession::new(&opts.zip_code, &proxy).map_err(|e| e.to_string())?
        }
    };

    let results = ScrapeEngine::scrape_rows(
        &mut session,
        vec![row],
        opts.request_interval_ms,
        1,
        std::sync::Arc::clone(&state.cancel_flag),
        on_progress,
        false,
    )
    .await
    .map_err(|e| crate::config::friendly_network_error(e))?;

    *state.session.lock() = Some(session);
    results
        .into_iter()
        .next()
        .ok_or_else(|| "刷新失败".to_string())
}

pub async fn refresh_all(
    state: &AppState,
    options: Option<ScrapeOptions>,
    on_progress: Option<ProgressCallback>,
) -> Result<Vec<RowResult>, String> {
    let rows = state.last_rows.lock().clone();
    if rows.is_empty() {
        return Err("没有可刷新的结果".to_string());
    }
    start_scrape(state, rows, options, on_progress).await
}

pub fn export_csv(rows: &[RowResult]) -> String {
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
        out.push_str(
            &row.price_value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
        );
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
    out
}

pub fn cancel_scrape(state: &AppState) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

pub async fn run_self_check(
    state: &AppState,
    zip_code: Option<String>,
) -> Result<SelfCheckResult, String> {
    let zip = zip_code.unwrap_or_else(|| crate::config::DEFAULT_ZIP.to_string());
    let proxy = state.proxy_config();
    let mut session = AmazonSession::new(&zip, &proxy).map_err(|e| e.to_string())?;
    let (ok, price_text, currency, message) = self_check(&mut session)
        .await
        .map_err(|e| crate::config::friendly_network_error(e))?;
    *state.session.lock() = Some(session);
    Ok(SelfCheckResult {
        ok,
        asin: crate::config::SELF_CHECK_ASIN.to_string(),
        price_text,
        currency,
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
