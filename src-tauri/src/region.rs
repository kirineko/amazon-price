use crate::config::{self, AMAZON_BASE, UNAVAILABLE_MARKERS};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::cookie::Jar;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, REFERER, USER_AGENT};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

const JPY_CURRENCY: &str = "JPY";

#[derive(Clone)]
pub struct AmazonSession {
    pub client: Client,
    cookie_jar: Arc<Jar>,
    pub zip_code: String,
    pub delivery_location: Option<String>,
}

impl AmazonSession {
    pub fn new(zip_code: &str) -> Result<Self> {
        if !config::is_valid_zip(zip_code) {
            anyhow::bail!("邮编格式无效，应为 123-4567");
        }

        let cookie_jar = Arc::new(Jar::default());
        let client = Client::builder()
            .cookie_provider(Arc::clone(&cookie_jar))
            .gzip(true)
            .brotli(true)
            .connect_timeout(Duration::from_secs(config::SESSION_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(config::SESSION_REQUEST_TIMEOUT_SECS))
            .build()
            .context("failed to build HTTP client")?;

        let session = Self {
            client,
            cookie_jar,
            zip_code: zip_code.to_string(),
            delivery_location: None,
        };
        session.seed_currency_cookies();
        Ok(session)
    }

    pub fn seed_currency_cookies(&self) {
        if let Ok(url) = reqwest::Url::parse("https://www.amazon.co.jp/") {
            self.cookie_jar.add_cookie_str("i18n-prefs=JPY", &url);
            self.cookie_jar.add_cookie_str("lc-acbjp=ja_JP", &url);
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.seed_currency_cookies();
        let token = self.fetch_glow_token().await?;
        self.set_delivery_zip(&token).await?;
        self.seed_currency_cookies();
        Ok(())
    }

    pub async fn init_with_retry(&mut self) -> Result<()> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..config::SESSION_INIT_RETRIES {
            match self.init().await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_err = Some(err);
                    if attempt + 1 < config::SESSION_INIT_RETRIES {
                        let delay = config::RETRY_DELAYS_MS
                            .get(attempt as usize)
                            .copied()
                            .unwrap_or(3200);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("会话初始化失败")))
    }

    async fn fetch_glow_token(&self) -> Result<String> {
        let urls = [
            format!("{AMAZON_BASE}/?language=ja_JP"),
            config::product_url(config::SELF_CHECK_ASIN),
        ];

        let mut last_err: Option<anyhow::Error> = None;
        for url in urls {
            match self.fetch_page_html(&url).await {
                Ok(html) => {
                    if is_waf_challenge(&html) {
                        last_err = Some(anyhow!("页面触发 AWS WAF 验证: {url}"));
                        continue;
                    }
                    if let Some(token) = extract_glow_token(&html) {
                        return Ok(token);
                    }
                    last_err = Some(anyhow!("未在页面中找到 glowValidationToken: {url}"));
                }
                Err(err) => {
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("未找到 glowValidationToken")))
    }

    async fn fetch_page_html(&self, url: &str) -> Result<String> {
        self.seed_currency_cookies();
        let response = self
            .client
            .get(url)
            .headers(default_headers(Some(AMAZON_BASE)))
            .send()
            .await
            .with_context(|| format!("failed to fetch page {url}"))?
            .error_for_status()
            .with_context(|| format!("page returned error status: {url}"))?;

        response
            .text()
            .await
            .with_context(|| format!("failed to read page body: {url}"))
    }

    async fn set_delivery_zip(&mut self, token: &str) -> Result<()> {
        if token.is_empty() {
            anyhow::bail!("设置配送地区失败：缺少 CSRF token（首页可能被 WAF 拦截）");
        }

        let url = format!("{AMAZON_BASE}/gp/delivery/ajax/address-change.html");
        let body = format!(
            "locationType=LOCATION_INPUT&zipCode={}&storeContext=generic&deviceType=web&pageType=Gateway&actionSource=glow&almBrandId=undefined",
            self.zip_code
        );

        let mut headers = default_headers(Some(AMAZON_BASE));
        headers.insert("anti-csrftoken-a2z", HeaderValue::from_str(token)?);
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/x-www-form-urlencoded;charset=UTF-8"),
        );
        headers.insert("x-requested-with", HeaderValue::from_static("XMLHttpRequest"));

        let response = self
            .client
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .context("failed to set delivery zip")?
            .error_for_status()
            .context("address-change returned error status")?;

        let text = response.text().await?;
        if text.trim().is_empty() {
            anyhow::bail!(
                "设置配送地区失败：Amazon 返回空响应（可能缺少有效 CSRF token 或会话未建立）"
            );
        }
        if !text.contains("\"successful\":1") && !text.contains("\"successful\": 1") {
            anyhow::bail!("设置配送地区失败: {text}");
        }
        if let Ok(parsed) = serde_json::from_str::<AddressChangeResponse>(&text) {
            if let Some(address) = parsed.address {
                let city = address.city.unwrap_or_default();
                let zip = address.zip_code.unwrap_or_else(|| self.zip_code.clone());
                self.delivery_location = Some(format!("{city} {zip}").trim().to_string());
            }
        }
        Ok(())
    }

    pub async fn fetch_delivery_location(&self) -> Result<Option<String>> {
        self.seed_currency_cookies();
        let response = self
            .client
            .get(AMAZON_BASE)
            .headers(default_headers(None))
            .send()
            .await?
            .error_for_status()?;

        let html = response.text().await?;
        Ok(extract_delivery_location(&html))
    }

    pub async fn fetch_search_html(&self, asin: &str) -> Result<String> {
        self.seed_currency_cookies();
        let url = config::search_url(asin);
        let response = self
            .client
            .get(&url)
            .headers(default_headers(Some(AMAZON_BASE)))
            .send()
            .await
            .with_context(|| format!("failed to fetch search page for {asin}"))?
            .error_for_status()
            .with_context(|| format!("search page returned error status for {asin}"))?;

        response
            .text()
            .await
            .with_context(|| format!("failed to read search body for {asin}"))
    }

    pub async fn fetch_product_html(&self, asin: &str) -> Result<String> {
        self.seed_currency_cookies();
        let url = config::product_url(asin);
        let response = self
            .client
            .get(&url)
            .headers(default_headers(Some(AMAZON_BASE)))
            .send()
            .await
            .with_context(|| format!("failed to fetch product page for {asin}"))?
            .error_for_status()
            .with_context(|| format!("product page returned error status for {asin}"))?;

        response
            .text()
            .await
            .with_context(|| format!("failed to read body for {asin}"))
    }

    pub async fn fetch_price(&self, asin: &str) -> Result<ParsedProduct> {
        self.seed_currency_cookies();

        if let Ok(html) = self.fetch_search_html(asin).await {
            if let Some(extracted) = parse_search_page(&html, asin) {
                return Ok(ParsedProduct {
                    price_text: extracted.text.clone(),
                    price_value: extracted.value,
                    currency: extracted.currency.clone(),
                    page_asin: Some(asin.to_uppercase()),
                    unavailable: false,
                });
            }
        }

        let html = self.fetch_product_html(asin).await?;
        Ok(parse_product_page(&html, asin))
    }

    pub fn region_looks_valid(&self) -> bool {
        self.delivery_location
            .as_ref()
            .map(|loc| loc.contains(&self.zip_code) || loc.contains("渋谷") || loc.contains("東京"))
            .unwrap_or(false)
    }
}

pub fn is_waf_challenge(html: &str) -> bool {
    html.contains("awsWaf") || html.contains("gokuProps") || html.contains("AwsWafIntegration")
}

pub fn extract_glow_token(html: &str) -> Option<String> {
    let patterns = [
        r#"(?i)id="glowValidationToken"[^>]*value="([^"]+)""#,
        r#"(?i)name="glow-validation-token"[^>]*value="([^"]+)""#,
        r#"(?i)"anti-csrftoken-a2z"\s*:\s*"([^"]+)""#,
    ];
    for pat in patterns {
        if let Ok(re) = Regex::new(pat) {
            if let Some(token) = re
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
            {
                if !token.is_empty() {
                    return Some(token);
                }
            }
        }
    }
    None
}

pub fn extract_delivery_location(html: &str) -> Option<String> {
    let re = Regex::new(r#"id="glow-ingress-line2"[^>]*>(.*?)</span>"#).ok()?;
    re.captures(html)
        .and_then(|c| c.get(1))
        .map(|m| {
            m.as_str()
                .replace("&zwnj;", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
}

pub fn default_headers(referer: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(config::USER_AGENTS[0]));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("ja-JP,ja;q=0.9,en-US;q=0.8,en;q=0.7"),
    );
    if let Some(referer) = referer {
        if let Ok(value) = HeaderValue::from_str(referer) {
            headers.insert(REFERER, value);
        }
    }
    headers
}

#[derive(Debug, Clone, Default)]
pub struct ExtractedPrice {
    pub text: Option<String>,
    pub value: Option<u64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedProduct {
    pub price_text: Option<String>,
    pub price_value: Option<u64>,
    pub currency: Option<String>,
    pub page_asin: Option<String>,
    pub unavailable: bool,
}

pub fn is_jpy_currency(currency: Option<&str>) -> bool {
    currency == Some(JPY_CURRENCY)
}

pub fn detect_currency_from_text(text: &str) -> Option<String> {
    let upper = text.to_uppercase();
    if text.contains('￥') {
        return Some(JPY_CURRENCY.to_string());
    }
    if upper.contains("HK$") || upper.contains("HKD") {
        return Some("HKD".to_string());
    }
    if upper.contains("USD") || text.contains('$') {
        return Some("USD".to_string());
    }
    if text.contains('€') || upper.contains("EUR") {
        return Some("EUR".to_string());
    }
    if text.contains('¥') {
        return Some("OTHER".to_string());
    }
    None
}

pub fn detect_currency_from_symbol(symbol: &str) -> Option<String> {
    let trimmed = symbol.trim();
    if trimmed.contains('￥') {
        return Some(JPY_CURRENCY.to_string());
    }
    if trimmed.contains("HK$") {
        return Some("HKD".to_string());
    }
    if trimmed.contains('$') {
        return Some("USD".to_string());
    }
    if trimmed.contains('€') {
        return Some("EUR".to_string());
    }
    if trimmed.contains('¥') {
        return Some("OTHER".to_string());
    }
    None
}

pub fn parse_search_page(html: &str, target_asin: &str) -> Option<ExtractedPrice> {
    let document = Html::parse_document(html);
    let card_sel = Selector::parse(r#"div[data-component-type="s-search-result"]"#).ok()?;
    let offscreen_sel = Selector::parse("span.a-offscreen").ok()?;
    let target = target_asin.to_uppercase();

    for card in document.select(&card_sel) {
        let card_asin = card.value().attr("data-asin")?;
        if card_asin.to_uppercase() != target {
            continue;
        }
        for el in card.select(&offscreen_sel) {
            let text = el.text().collect::<String>().trim().to_string();
            if text.is_empty() {
                continue;
            }
            if let Some(currency) = detect_currency_from_text(&text) {
                return Some(ExtractedPrice {
                    text: Some(text.clone()),
                    value: parse_price_value(&text),
                    currency: Some(currency),
                });
            }
        }
    }
    None
}

pub fn parse_product_page(html: &str, _expected_asin: &str) -> ParsedProduct {
    let unavailable = UNAVAILABLE_MARKERS.iter().any(|m| html.contains(m));
    let page_asin = extract_page_asin(html);
    let extracted = extract_price(html);

    ParsedProduct {
        price_text: extracted.text.clone(),
        price_value: extracted.value,
        currency: extracted.currency.clone(),
        page_asin,
        unavailable: unavailable && extracted.text.is_none(),
    }
}

fn extract_page_asin(html: &str) -> Option<String> {
    if let Ok(re) = Regex::new(
        r#"(?i)rel="canonical" href="https://www\.amazon\.co\.jp/(?:dp|gp/product)/([A-Z0-9]{10})""#,
    ) {
        if let Some(caps) = re.captures(html) {
            return caps.get(1).map(|m| m.as_str().to_uppercase());
        }
    }

    if let Ok(re) = Regex::new(r#"data-asin="([A-Z0-9]{10})""#) {
        for caps in re.captures_iter(html) {
            if let Some(m) = caps.get(1) {
                let asin = m.as_str().to_uppercase();
                if !asin.is_empty() {
                    return Some(asin);
                }
            }
        }
    }

    None
}

fn extract_price(html: &str) -> ExtractedPrice {
    if let Some(found) = extract_first_offscreen_price(html) {
        return found;
    }

    let document = Html::parse_document(html);
    for id in config::CORE_PRICE_IDS {
        if let Some(found) = extract_price_in_element(&document, id) {
            return found;
        }
    }

    extract_whole_fraction_price(&document)
}

fn extract_first_offscreen_price(html: &str) -> Option<ExtractedPrice> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("span.a-offscreen").ok()?;
    for el in document.select(&selector) {
        let text = el.text().collect::<String>().trim().to_string();
        if text.is_empty() {
            continue;
        }
        if let Some(currency) = detect_currency_from_text(&text) {
            return Some(ExtractedPrice {
                text: Some(text.clone()),
                value: parse_price_value(&text),
                currency: Some(currency),
            });
        }
    }
    None
}

fn extract_price_in_element(document: &Html, element_id: &str) -> Option<ExtractedPrice> {
    let selector = Selector::parse(&format!("#{element_id} span.a-offscreen")).ok()?;
    for el in document.select(&selector) {
        let text = el.text().collect::<String>().trim().to_string();
        if text.is_empty() {
            continue;
        }
        if let Some(currency) = detect_currency_from_text(&text) {
            return Some(ExtractedPrice {
                text: Some(text.clone()),
                value: parse_price_value(&text),
                currency: Some(currency),
            });
        }
    }
    None
}

fn extract_whole_fraction_price(document: &Html) -> ExtractedPrice {
    let whole_sel = Selector::parse("span.a-price-whole").ok();
    let frac_sel = Selector::parse("span.a-price-fraction").ok();
    let symbol_sel = Selector::parse("span.a-price-symbol").ok();

    let symbol_text = symbol_sel
        .as_ref()
        .and_then(|sel| document.select(sel).next())
        .map(|el| el.text().collect::<String>());

    let currency = symbol_text
        .as_deref()
        .and_then(detect_currency_from_symbol);

    if let (Some(whole_sel), Some(frac_sel)) = (whole_sel, frac_sel) {
        if let (Some(whole), frac) = (
            document.select(&whole_sel).next(),
            document.select(&frac_sel).next(),
        ) {
            let whole_text = whole.text().collect::<String>().replace(',', "");
            let frac_text = frac.map(|f| f.text().collect::<String>()).unwrap_or_default();
            if !whole_text.trim().is_empty() {
                let prefix = symbol_text
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "￥".to_string());
                let text = format!("{prefix}{whole_text}{frac_text}");
                return ExtractedPrice {
                    text: Some(text.clone()),
                    value: parse_price_value(&text),
                    currency,
                };
            }
        }
    }

    ExtractedPrice::default()
}

pub fn parse_price_value(text: &str) -> Option<u64> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

pub fn parse_yen_value(text: &str) -> Option<u64> {
    parse_price_value(text)
}

pub fn batch_plan(scrapeable_count: usize) -> (bool, usize) {
    if scrapeable_count > config::BATCH_SIZE {
        let batch_total = scrapeable_count.div_ceil(config::BATCH_SIZE);
        (true, batch_total)
    } else {
        (false, 1)
    }
}

#[derive(Debug, Deserialize)]
struct AddressChangeResponse {
    address: Option<AddressInfo>,
}

#[derive(Debug, Deserialize)]
struct AddressInfo {
    city: Option<String>,
    #[serde(rename = "zipCode")]
    zip_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yen_value() {
        assert_eq!(parse_yen_value("￥3,951"), Some(3951));
    }

    #[test]
    fn detects_waf_challenge_page() {
        let html = r#"<!DOCTYPE html><script>window.gokuProps={};AwsWafIntegration.getToken()</script>"#;
        assert!(is_waf_challenge(html));
    }

    #[test]
    fn extracts_token_from_product_page_fixture() {
        let html = r#"<input id="glowValidationToken" name="glow-validation-token" type="hidden" value="abc123" />"#;
        assert!(!is_waf_challenge(html));
        assert_eq!(extract_glow_token(html), Some("abc123".to_string()));
    }

    #[test]
    fn extracts_token() {
        let html = r#"<input id="glowValidationToken" value="abc123" />"#;
        assert_eq!(extract_glow_token(html), Some("abc123".to_string()));
    }

    #[test]
    fn detects_jpy_from_fullwidth_yen() {
        assert_eq!(
            detect_currency_from_text("￥5,287"),
            Some("JPY".to_string())
        );
        assert!(is_jpy_currency(detect_currency_from_text("￥5,287").as_deref()));
    }

    #[test]
    fn detects_usd_and_hkd() {
        assert_eq!(
            detect_currency_from_text("USD 36.20"),
            Some("USD".to_string())
        );
        assert_eq!(
            detect_currency_from_text("HK$123"),
            Some("HKD".to_string())
        );
        assert!(!is_jpy_currency(Some("USD")));
    }

    #[test]
    fn detects_halfwidth_yen_as_non_jpy() {
        assert_eq!(
            detect_currency_from_text("¥123"),
            Some("OTHER".to_string())
        );
        assert!(!is_jpy_currency(Some("OTHER")));
    }

    #[test]
    fn parse_search_page_matches_target_asin_only() {
        let html = r#"
        <div data-component-type="s-search-result" data-asin="B0OTHER123">
          <span class="a-offscreen">USD 99.99</span>
        </div>
        <div data-component-type="s-search-result" data-asin="B08CKGRHLF">
          <span class="a-offscreen">￥5,287</span>
        </div>
        "#;
        let parsed = parse_search_page(html, "b08ckgrhlf").expect("should match");
        assert_eq!(parsed.currency.as_deref(), Some("JPY"));
        assert_eq!(parsed.value, Some(5287));
    }

    #[test]
    fn parse_search_page_no_match_returns_none() {
        let html = r#"
        <div data-component-type="s-search-result" data-asin="B0OTHER123">
          <span class="a-offscreen">￥1,000</span>
        </div>
        "#;
        assert!(parse_search_page(html, "B08CKGRHLF").is_none());
    }

    #[test]
    fn whole_fraction_uses_real_symbol_not_assumed_jpy() {
        let html = r#"
        <span class="a-price-symbol">$</span>
        <span class="a-price-whole">36</span>
        <span class="a-price-fraction">20</span>
        "#;
        let document = Html::parse_document(html);
        let extracted = extract_whole_fraction_price(&document);
        assert_eq!(extracted.currency.as_deref(), Some("USD"));
        assert!(!is_jpy_currency(extracted.currency.as_deref()));
    }

    #[test]
    fn batch_plan_splits_over_100() {
        let (enabled, total) = batch_plan(250);
        assert!(enabled);
        assert_eq!(total, 3);
        let (enabled, total) = batch_plan(100);
        assert!(!enabled);
        assert_eq!(total, 1);
    }
}
