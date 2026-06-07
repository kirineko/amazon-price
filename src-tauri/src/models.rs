use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowResult {
    pub sku: String,
    pub dp_code: String,
    pub asin: String,
    pub amazon_url: String,
    pub price_text: Option<String>,
    pub price_value: Option<u64>,
    pub currency: String,
    pub status: RowStatus,
    pub error: Option<String>,
    pub fetched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RowStatus {
    Pending,
    Success,
    Unavailable,
    NoPrice,
    Mismatch,
    FormatError,
    Failed,
}

impl RowStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::Success => "成功",
            Self::Unavailable => "不可售",
            Self::NoPrice => "无价",
            Self::Mismatch => "疑似不匹配",
            Self::FormatError => "格式错误",
            Self::Failed => "失败",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScrapeOptions {
    pub zip_code: String,
    pub rate_per_sec: u32,
    pub concurrency: usize,
}

impl Default for ScrapeOptions {
    fn default() -> Self {
        Self {
            zip_code: crate::config::DEFAULT_ZIP.to_string(),
            rate_per_sec: crate::config::DEFAULT_RATE_PER_SEC,
            concurrency: crate::config::DEFAULT_CONCURRENCY,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeProgress {
    pub done: usize,
    pub total: usize,
    pub row: RowResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub initialized: bool,
    pub zip_code: String,
    pub delivery_location: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfCheckResult {
    pub ok: bool,
    pub asin: String,
    pub price_text: Option<String>,
    pub message: String,
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn build_amazon_url(asin: &str) -> String {
    crate::config::product_url(asin)
}

pub fn empty_row(sku: String, dp_code: String, asin: String, status: RowStatus, error: Option<String>) -> RowResult {
    RowResult {
        amazon_url: build_amazon_url(&asin),
        sku,
        dp_code,
        asin,
        price_text: None,
        price_value: None,
        currency: "JPY".to_string(),
        status,
        error,
        fetched_at: None,
    }
}
