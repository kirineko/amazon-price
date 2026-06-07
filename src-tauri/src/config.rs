pub const AMAZON_BASE: &str = "https://www.amazon.co.jp";
pub const DEFAULT_ZIP: &str = "150-0001";
pub const DEFAULT_RATE_PER_SEC: u32 = 3;
pub const DEFAULT_CONCURRENCY: usize = 3;
pub const MAX_RETRIES: u32 = 3;
pub const RETRY_DELAYS_MS: [u64; 3] = [800, 1600, 3200];
pub const SELF_CHECK_ASIN: &str = "B0DFXQWPPS";

pub const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
];

pub const UNAVAILABLE_MARKERS: &[&str] = &[
    "お取り扱いできません",
    "在庫切れ",
    "現在ご注文いただけません",
    "ただいまお取り扱いできません",
];

pub const PRICE_SELECTORS: &[&str] = &["span.a-offscreen"];
pub const CORE_PRICE_IDS: &[&str] = &[
    "corePriceDisplay_desktop_feature_div",
    "corePrice_feature_div",
    "corePrice_desktop_feature_div",
];

pub fn product_url(asin: &str) -> String {
    format!("{AMAZON_BASE}/dp/{asin}?th=1")
}

pub fn is_valid_zip(zip: &str) -> bool {
    regex::Regex::new(r"^\d{3}-\d{4}$")
        .map(|re| re.is_match(zip))
        .unwrap_or(false)
}
