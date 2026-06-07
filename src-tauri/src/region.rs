use crate::config::{self, AMAZON_BASE, UNAVAILABLE_MARKERS};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, REFERER, USER_AGENT};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone)]
pub struct AmazonSession {
    pub client: Client,
    pub zip_code: String,
    pub delivery_location: Option<String>,
}

impl AmazonSession {
    pub fn new(zip_code: &str) -> Result<Self> {
        if !config::is_valid_zip(zip_code) {
            anyhow::bail!("邮编格式无效，应为 123-4567");
        }

        let client = Client::builder()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .connect_timeout(Duration::from_secs(config::SESSION_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(config::SESSION_REQUEST_TIMEOUT_SECS))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            zip_code: zip_code.to_string(),
            delivery_location: None,
        })
    }

    pub async fn init(&mut self) -> Result<()> {
        let token = self.fetch_glow_token().await.unwrap_or_default();
        self.set_delivery_zip(&token).await?;
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
        let url = format!("{AMAZON_BASE}/?language=ja_JP");
        let response = self
            .client
            .get(&url)
            .headers(default_headers(None))
            .header("Cookie", "lc-acbjp=ja_JP; i18n-prefs=JPY")
            .send()
            .await
            .context("failed to fetch Amazon homepage")?
            .error_for_status()
            .context("homepage returned error status")?;

        let html = response.text().await?;
        extract_glow_token(&html).ok_or_else(|| anyhow!("未找到 glowValidationToken"))
    }

    async fn set_delivery_zip(&mut self, token: &str) -> Result<()> {
        let url = format!("{AMAZON_BASE}/gp/delivery/ajax/address-change.html");
        let body = format!(
            "locationType=LOCATION_INPUT&zipCode={}&storeContext=generic&deviceType=web&pageType=Gateway&actionSource=glow",
            self.zip_code
        );

        let mut headers = default_headers(Some(AMAZON_BASE));
        if !token.is_empty() {
            headers.insert("anti-csrftoken-a2z", HeaderValue::from_str(token)?);
        }
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
            anyhow::bail!("设置配送地区失败：Amazon 返回空响应");
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

    pub async fn fetch_product_html(&self, asin: &str) -> Result<String> {
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

        response.text().await.with_context(|| format!("failed to read body for {asin}"))
    }

    pub fn region_looks_valid(&self) -> bool {
        self.delivery_location
            .as_ref()
            .map(|loc| loc.contains(&self.zip_code) || loc.contains("渋谷") || loc.contains("東京"))
            .unwrap_or(false)
    }
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

#[derive(Debug, Clone)]
pub struct ParsedProduct {
    pub price_text: Option<String>,
    pub price_value: Option<u64>,
    pub page_asin: Option<String>,
    pub unavailable: bool,
}

pub fn parse_product_page(html: &str, expected_asin: &str) -> ParsedProduct {
    let unavailable = UNAVAILABLE_MARKERS.iter().any(|m| html.contains(m));
    let page_asin = extract_page_asin(html);
    let (price_text, price_value) = extract_price(html);

    let _mismatch = page_asin
        .as_ref()
        .map(|asin| !asin.eq_ignore_ascii_case(expected_asin))
        .unwrap_or(false);

    ParsedProduct {
        price_text: price_text.clone(),
        price_value,
        page_asin,
        unavailable: unavailable && price_text.is_none(),
    }
}

fn extract_page_asin(html: &str) -> Option<String> {
    if let Ok(re) = Regex::new(r#"(?i)rel="canonical" href="https://www\.amazon\.co\.jp/(?:dp|gp/product)/([A-Z0-9]{10})""#) {
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

fn extract_price(html: &str) -> (Option<String>, Option<u64>) {
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

fn extract_first_offscreen_price(html: &str) -> Option<(Option<String>, Option<u64>)> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("span.a-offscreen").ok()?;
    for el in document.select(&selector) {
        let text = el.text().collect::<String>().trim().to_string();
        if !text.is_empty() && (text.contains('￥') || text.contains('¥')) {
            return Some((Some(text.clone()), parse_yen_value(&text)));
        }
    }
    None
}

fn extract_price_in_element(document: &Html, element_id: &str) -> Option<(Option<String>, Option<u64>)> {
    let selector = Selector::parse(&format!("#{element_id} span.a-offscreen")).ok()?;
    for el in document.select(&selector) {
        let text = el.text().collect::<String>().trim().to_string();
        if !text.is_empty() && (text.contains('￥') || text.contains('¥')) {
            return Some((Some(text.clone()), parse_yen_value(&text)));
        }
    }
    None
}

fn extract_whole_fraction_price(document: &Html) -> (Option<String>, Option<u64>) {
    let whole_sel = Selector::parse("span.a-price-whole").ok();
    let frac_sel = Selector::parse("span.a-price-fraction").ok();

    if let (Some(whole_sel), Some(frac_sel)) = (whole_sel, frac_sel) {
        if let (Some(whole), frac) = (
            document.select(&whole_sel).next(),
            document.select(&frac_sel).next(),
        ) {
            let whole_text = whole.text().collect::<String>().replace(',', "");
            let frac_text = frac.map(|f| f.text().collect::<String>()).unwrap_or_default();
            if !whole_text.trim().is_empty() {
                let text = format!("￥{whole_text}{frac_text}");
                return (Some(text.clone()), parse_yen_value(&text));
            }
        }
    }

    (None, None)
}

pub fn parse_yen_value(text: &str) -> Option<u64> {
    let digits: String = text
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
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
    fn extracts_token() {
        let html = r#"<input id="glowValidationToken" value="abc123" />"#;
        assert_eq!(extract_glow_token(html), Some("abc123".to_string()));
    }
}
