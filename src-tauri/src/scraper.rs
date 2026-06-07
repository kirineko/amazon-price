use crate::config::{self, BATCH_COOLDOWN_SECS, BATCH_SIZE, RETRY_DELAYS_MS};
use crate::models::{RowResult, RowStatus, ScrapePhase, ScrapeProgress};
use crate::region::{is_jpy_currency, AmazonSession};
use anyhow::Result;
use governor::{Quota, RateLimiter};
use rand::Rng;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub type ProgressCallback = Arc<dyn Fn(ScrapeProgress) + Send + Sync>;

pub struct ScrapeEngine;

impl ScrapeEngine {
    pub async fn scrape_rows(
        session: &mut AmazonSession,
        rows: Vec<RowResult>,
        rate_per_sec: u32,
        concurrency: usize,
        cancel_flag: Arc<AtomicBool>,
        on_progress: Option<ProgressCallback>,
        enable_batching: bool,
    ) -> Result<Vec<RowResult>> {
        session.init_with_retry().await?;
        let shared_session = Arc::new(session.clone());

        let total = rows.len();
        let mut output: Vec<Option<RowResult>> = vec![None; total];
        let done_counter = Arc::new(AtomicUsize::new(0));

        let mut format_error_items: Vec<(usize, RowResult)> = Vec::new();
        let mut scrape_items: Vec<(usize, RowResult)> = Vec::new();

        for (index, row) in rows.into_iter().enumerate() {
            if row.status == RowStatus::FormatError {
                format_error_items.push((index, row));
            } else {
                scrape_items.push((index, row));
            }
        }

        for (index, row) in format_error_items {
            output[index] = Some(row.clone());
            let done = done_counter.fetch_add(1, Ordering::SeqCst) + 1;
            emit_progress(&on_progress, done, total, row, None);
        }

        let scrapeable_count = scrape_items.len();
        let use_batching = enable_batching && scrapeable_count > BATCH_SIZE;
        let batch_total = if use_batching {
            scrapeable_count.div_ceil(BATCH_SIZE)
        } else {
            1
        };

        let chunk_size = if use_batching {
            BATCH_SIZE
        } else {
            scrape_items.len().max(1)
        };
        let batches: Vec<Vec<(usize, RowResult)>> = scrape_items
            .chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        for (batch_idx, batch) in batches.into_iter().enumerate() {
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }

            scrape_batch(
                &batch,
                &mut output,
                &shared_session,
                rate_per_sec,
                concurrency,
                Arc::clone(&cancel_flag),
                on_progress.clone(),
                Arc::clone(&done_counter),
                total,
            )
            .await;

            let is_last_batch = batch_idx + 1 >= batch_total;
            if use_batching && !is_last_batch {
                if !cooldown_between_batches(
                    &on_progress,
                    done_counter.load(Ordering::SeqCst),
                    total,
                    batch_idx as u32 + 1,
                    batch_total as u32,
                    Arc::clone(&cancel_flag),
                )
                .await
                {
                    break;
                }
            }
        }

        for (index, row) in scrape_items {
            if output[index].is_none() {
                mark_one_cancelled(index, &row, &mut output, &on_progress, &done_counter, total);
            }
        }

        Ok(output.into_iter().flatten().collect())
    }
}

async fn scrape_batch(
    batch: &[(usize, RowResult)],
    output: &mut [Option<RowResult>],
    session: &Arc<AmazonSession>,
    rate_per_sec: u32,
    concurrency: usize,
    cancel_flag: Arc<AtomicBool>,
    on_progress: Option<ProgressCallback>,
    done_counter: Arc<AtomicUsize>,
    total: usize,
) {
    let quota = Quota::per_second(NonZeroU32::new(rate_per_sec.max(1)).unwrap());
    let limiter = Arc::new(RateLimiter::direct(quota));
    let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut join_set = JoinSet::new();

    for (index, row) in batch.iter().cloned() {
        if cancel_flag.load(Ordering::SeqCst) {
            let mut cancelled = row.clone();
            cancelled.status = RowStatus::Failed;
            cancelled.error = Some("已取消".to_string());
            output[index] = Some(cancelled.clone());
            let done = done_counter.fetch_add(1, Ordering::SeqCst) + 1;
            emit_progress(&on_progress, done, total, cancelled, None);
            continue;
        }

        let limiter = Arc::clone(&limiter);
        let semaphore = Arc::clone(&semaphore);
        let session = Arc::clone(session);
        let cancel_flag = Arc::clone(&cancel_flag);
        let on_progress = on_progress.clone();
        let done_counter = Arc::clone(&done_counter);

        join_set.spawn(async move {
            let _permit = semaphore.acquire().await.expect("semaphore");
            limiter.until_ready().await;
            let jitter = rand::thread_rng().gen_range(50..=150);
            tokio::time::sleep(Duration::from_millis(jitter)).await;
            let result = scrape_one_with_retry(&session, &row, cancel_flag).await;

            let done = done_counter.fetch_add(1, Ordering::SeqCst) + 1;
            emit_progress(&on_progress, done, total, result.clone(), None);
            (index, result)
        });
    }

    while let Some(res) = join_set.join_next().await {
        match res {
            Ok((index, row)) => output[index] = Some(row),
            Err(err) => eprintln!("scrape task failed: {err}"),
        }
    }
}

async fn cooldown_between_batches(
    on_progress: &Option<ProgressCallback>,
    done: usize,
    total: usize,
    batch_index: u32,
    batch_total: u32,
    cancel_flag: Arc<AtomicBool>,
) -> bool {
    let cooldown = Duration::from_secs(BATCH_COOLDOWN_SECS);
    let started = Instant::now();

    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return false;
        }

        let elapsed = started.elapsed();
        if elapsed >= cooldown {
            return true;
        }

        let remaining = (cooldown - elapsed).as_secs().max(1) as u32;
        emit_cooling_progress(
            on_progress,
            done,
            total,
            batch_index,
            batch_total,
            remaining,
        );

        let tick = Duration::from_secs(1);
        let wait_for = cooldown.saturating_sub(elapsed).min(tick);

        tokio::select! {
            _ = tokio::time::sleep(wait_for) => {}
            _ = async {
                loop {
                    if cancel_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } => {
                return false;
            }
        }
    }
}

fn mark_one_cancelled(
    index: usize,
    row: &RowResult,
    output: &mut [Option<RowResult>],
    on_progress: &Option<ProgressCallback>,
    done_counter: &Arc<AtomicUsize>,
    total: usize,
) {
    let mut cancelled = row.clone();
    cancelled.status = RowStatus::Failed;
    cancelled.error = Some("已取消".to_string());
    output[index] = Some(cancelled.clone());
    let done = done_counter.fetch_add(1, Ordering::SeqCst) + 1;
    emit_progress(on_progress, done, total, cancelled, None);
}

fn emit_progress(
    on_progress: &Option<ProgressCallback>,
    done: usize,
    total: usize,
    row: RowResult,
    phase: Option<ScrapePhase>,
) {
    if let Some(cb) = on_progress {
        cb(ScrapeProgress {
            done,
            total,
            row,
            phase,
            batch_index: None,
            batch_total: None,
            cooldown_secs: None,
        });
    }
}

fn emit_cooling_progress(
    on_progress: &Option<ProgressCallback>,
    done: usize,
    total: usize,
    batch_index: u32,
    batch_total: u32,
    cooldown_secs: u32,
) {
    if let Some(cb) = on_progress {
        cb(ScrapeProgress {
            done,
            total,
            row: RowResult {
                sku: String::new(),
                dp_code: String::new(),
                asin: String::new(),
                amazon_url: String::new(),
                price_text: None,
                price_value: None,
                currency: String::new(),
                status: RowStatus::Pending,
                error: None,
                fetched_at: None,
            },
            phase: Some(ScrapePhase::Cooling),
            batch_index: Some(batch_index),
            batch_total: Some(batch_total),
            cooldown_secs: Some(cooldown_secs),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batching_enabled_only_when_over_threshold() {
        assert!(250 > BATCH_SIZE);
        assert!(!(50 > BATCH_SIZE));
    }
}
