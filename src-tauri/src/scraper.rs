use crate::config::{self, RETRY_DELAYS_MS};
use crate::models::{RowResult, RowStatus, ScrapeProgress};
use crate::region::{parse_product_page, AmazonSession};
use anyhow::Result;
use governor::{Quota, RateLimiter};
use rand::Rng;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub struct ScrapeEngine;

impl ScrapeEngine {
    pub async fn scrape_rows(
        session: &mut AmazonSession,
        rows: Vec<RowResult>,
        rate_per_sec: u32,
        concurrency: usize,
        cancel_flag: Arc<AtomicBool>,
        app: AppHandle,
    ) -> Result<Vec<RowResult>> {
        session.init().await?;
        let shared_session = Arc::new(session.clone());

        let total = rows.len();
        let quota = Quota::per_second(NonZeroU32::new(rate_per_sec.max(1)).unwrap());
        let limiter = Arc::new(RateLimiter::direct(quota));
        let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
        let done_counter = Arc::new(AtomicUsize::new(0));

        let mut output: Vec<Option<RowResult>> = vec![None; total];
        let mut join_set = JoinSet::new();

        for (index, row) in rows.into_iter().enumerate() {
            if row.status == RowStatus::FormatError {
                output[index] = Some(row.clone());
                let done = done_counter.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = app.emit(
                    "scrape-progress",
                    ScrapeProgress {
                        done,
                        total,
                        row,
                    },
                );
                continue;
            }

            let limiter = Arc::clone(&limiter);
            let semaphore = Arc::clone(&semaphore);
            let session = Arc::clone(&shared_session);
            let cancel_flag = Arc::clone(&cancel_flag);
            let app = app.clone();
            let done_counter = Arc::clone(&done_counter);

            join_set.spawn(async move {
                let result = if cancel_flag.load(Ordering::SeqCst) {
                    let mut cancelled = row.clone();
                    cancelled.status = RowStatus::Failed;
                    cancelled.error = Some("已取消".to_string());
                    cancelled
                } else {
                    let _permit = semaphore.acquire().await.expect("semaphore");
                    limiter.until_ready().await;
                    let jitter = rand::thread_rng().gen_range(50..=150);
                    tokio::time::sleep(Duration::from_millis(jitter)).await;
                    scrape_one_with_retry(&session, &row, cancel_flag).await
                };

                let done = done_counter.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = app.emit(
                    "scrape-progress",
                    ScrapeProgress {
                        done,
                        total,
                        row: result.clone(),
                    },
                );
                (index, result)
            });
        }

        while let Some(res) = join_set.join_next().await {
            match res {
                Ok((index, row)) => output[index] = Some(row),
                Err(err) => eprintln!("scrape task failed: {err}"),
            }
        }

        Ok(output.into_iter().flatten().collect())
    }
}

async fn scrape_one_with_retry(
    session: &Arc<AmazonSession>,
    row: &RowResult,
    cancel_flag: Arc<AtomicBool>,
) -> RowResult {
    let mut current = row.clone();
    current.status = RowStatus::Pending;
    current.error = None;

    for attempt in 0..=config::MAX_RETRIES {
        if cancel_flag.load(Ordering::SeqCst) {
            current.status = RowStatus::Failed;
            current.error = Some("已取消".to_string());
            return current;
        }

        match fetch_and_parse(session, &current.asin).await {
            Ok(parsed) => {
                current.fetched_at = Some(crate::models::now_iso());

                if let Some(page_asin) = parsed.page_asin {
                    if !page_asin.eq_ignore_ascii_case(&current.asin) {
                        current.status = RowStatus::Mismatch;
                        current.error = Some(format!("页面 ASIN 为 {page_asin}"));
                    }
                }

                if let Some(price_text) = parsed.price_text {
                    current.price_text = Some(price_text.clone());
                    current.price_value = parsed.price_value;
                    current.status = RowStatus::Success;
                    current.error = None;
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

async fn fetch_and_parse(
    session: &Arc<AmazonSession>,
    asin: &str,
) -> Result<crate::region::ParsedProduct> {
    let html = session.fetch_product_html(asin).await?;
    Ok(parse_product_page(&html, asin))
}

pub async fn self_check(session: &mut AmazonSession) -> Result<(bool, Option<String>, String)> {
    session.init().await?;
    let html = session
        .fetch_product_html(config::SELF_CHECK_ASIN)
        .await?;
    let parsed = parse_product_page(&html, config::SELF_CHECK_ASIN);
    if let Some(price) = parsed.price_text {
        Ok((true, Some(price.clone()), "自检通过".to_string()))
    } else {
        Ok((false, None, "自检失败：未能获取测试商品价格".to_string()))
    }
}

pub struct RateProbe {
    started: Instant,
    count: usize,
}

impl RateProbe {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
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
