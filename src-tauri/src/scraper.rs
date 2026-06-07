use crate::config::{self, RETRY_DELAYS_MS};
use crate::models::{RowResult, RowStatus, ScrapeProgress};
use crate::region::{is_jpy_currency, AmazonSession};
use anyhow::Result;
use governor::{Quota, RateLimiter};
use rand::Rng;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub type ProgressCallback = Arc<dyn Fn(ScrapeProgress) + Send + Sync>;

pub struct ScrapeEngine;

impl ScrapeEngine {
    pub async fn scrape_rows(
        session: &mut AmazonSession,
        rows: Vec<RowResult>,
        request_interval_ms: u64,
        cancel_flag: Arc<AtomicBool>,
        on_progress: Option<ProgressCallback>,
    ) -> Result<Vec<RowResult>> {
        session.init_with_retry().await?;
        let shared_session = Arc::new(session.clone());

        let total = rows.len();
        let mut output = Vec::with_capacity(total);
        let mut done_counter = 0usize;

        let period_ms = request_interval_ms.max(500);
        let quota = Quota::with_period(Duration::from_millis(period_ms))
            .expect("valid quota period")
            .allow_burst(NonZeroU32::new(1).unwrap());
        let limiter = RateLimiter::direct(quota);

        for row in rows {
            if row.status == RowStatus::FormatError {
                output.push(row.clone());
                done_counter += 1;
                emit_progress(&on_progress, done_counter, total, row);
                continue;
            }

            if cancel_flag.load(Ordering::SeqCst) {
                output.push(row);
                continue;
            }

            limiter.until_ready().await;
            let jitter = rand::thread_rng().gen_range(50..=150);
            tokio::time::sleep(Duration::from_millis(jitter)).await;

            let result = scrape_one_with_retry(&shared_session, &row).await;
            done_counter += 1;
            emit_progress(&on_progress, done_counter, total, result.clone());
            output.push(result);
        }

        Ok(output)
    }
}

fn emit_progress(
    on_progress: &Option<ProgressCallback>,
    done: usize,
    total: usize,
    row: RowResult,
) {
    if let Some(cb) = on_progress {
        cb(ScrapeProgress { done, total, row });
    }
}

async fn scrape_one_with_retry(session: &Arc<AmazonSession>, row: &RowResult) -> RowResult {
    let mut current = row.clone();
    current.amazon_url = config::search_url(&current.asin);
    current.status = RowStatus::Pending;
    current.error = None;

    for attempt in 0..=config::MAX_RETRIES {
        match session.fetch_price(&current.asin).await {
            Ok(parsed) => {
                current.fetched_at = Some(crate::models::now_iso());

                if let Some(page_asin) = parsed.page_asin {
                    if !page_asin.eq_ignore_ascii_case(&current.asin) {
                        current.status = RowStatus::Mismatch;
                        current.error = Some(format!("页面 ASIN 为 {page_asin}"));
                    }
                }

                if let Some(price_text) = parsed.price_text {
                    let currency = parsed
                        .currency
                        .clone()
                        .unwrap_or_else(|| "UNKNOWN".to_string());
                    current.price_text = Some(price_text.clone());
                    current.price_value = parsed.price_value;
                    current.currency = currency.clone();

                    if !is_jpy_currency(Some(&currency)) {
                        current.status = RowStatus::Failed;
                        current.error =
                            Some(format!("货币未锁定：检测到 {currency}（{price_text}）"));
                        return current;
                    }

                    if current.status != RowStatus::Mismatch {
                        current.status = RowStatus::Success;
                        current.error = None;
                    }
                    return current;
                }

                if parsed.unavailable {
                    current.status = RowStatus::Unavailable;
                    current.error = Some("商品当前不可售".to_string());
                    return current;
                }

                current.status = RowStatus::NoPrice;
                current.error = Some("页面未找到价格".to_string());
            }
            Err(err) => {
                current.status = RowStatus::Failed;
                current.error = Some(format!("{err:#}"));
            }
        }

        if attempt < config::MAX_RETRIES {
            let delay = RETRY_DELAYS_MS
                .get(attempt as usize)
                .copied()
                .unwrap_or(3200);
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }

    current
}

pub async fn self_check(
    session: &mut AmazonSession,
) -> Result<(bool, Option<String>, Option<String>, String)> {
    session.init_with_retry().await?;
    let parsed = session
        .fetch_price(config::SELF_CHECK_ASIN)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;

    let currency = parsed.currency.clone();
    if let Some(price) = parsed.price_text {
        if is_jpy_currency(currency.as_deref()) {
            Ok((
                true,
                Some(price.clone()),
                currency,
                "自检通过".to_string(),
            ))
        } else {
            let detected = currency.unwrap_or_else(|| "UNKNOWN".to_string());
            Ok((
                false,
                Some(price.clone()),
                Some(detected.clone()),
                format!("货币未锁定：检测到 {detected}（{price}）"),
            ))
        }
    } else {
        Ok((
            false,
            None,
            currency,
            "自检失败：未能获取测试商品价格".to_string(),
        ))
    }
}

pub struct RateProbe {
    started: std::time::Instant,
    count: usize,
}

impl RateProbe {
    pub fn new() -> Self {
        Self {
            started: std::time::Instant::now(),
            count: 0,
        }
    }

    pub fn tick(&mut self) {
        self.count += 1;
    }

    pub fn rate_per_sec(&self) -> f64 {
        let elapsed = self.started.elapsed().as_secs_f64().max(0.001);
        self.count as f64 / elapsed
    }
}
