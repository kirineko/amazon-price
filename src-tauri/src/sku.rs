use crate::config;
use crate::models::{RowResult, RowStatus};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkuError {
    #[error("empty line")]
    Empty,
    #[error("invalid ASIN format: {0}")]
    InvalidFormat(String),
}

#[derive(Debug, Clone)]
pub struct ParsedSku {
    pub sku: String,
    pub dp_code: String,
    pub asin: String,
}

pub fn parse_dp_code(raw: &str) -> Result<(String, String), SkuError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SkuError::Empty);
    }

    let mut body = trimmed.to_string();
    if body.len() >= 3 && body[..3].eq_ignore_ascii_case("gx-") {
        body = body[3..].to_string();
    }

    if body.len() == 13 && body[10..].chars().all(|c| c.is_ascii_digit()) {
        body = body[..10].to_string();
    }

    let dp_code = body;
    let asin = dp_code.to_uppercase();

    if !is_valid_asin(&asin) {
        return Err(SkuError::InvalidFormat(asin));
    }

    Ok((dp_code, asin))
}

pub fn is_valid_asin(asin: &str) -> bool {
    asin.len() == 10 && asin.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn parse_sku_line(line: &str) -> Result<ParsedSku, SkuError> {
    let sku = line.trim().to_string();
    let (dp_code, asin) = parse_dp_code(&sku)?;
    Ok(ParsedSku { sku, dp_code, asin })
}

pub fn parse_skus_from_text(text: &str) -> (Vec<RowResult>, usize) {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    let mut duplicate_count = 0;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match parse_sku_line(trimmed) {
            Ok(parsed) => {
                let key = parsed.asin.clone();
                if !seen.insert(key) {
                    duplicate_count += 1;
                    continue;
                }
                rows.push(RowResult {
                    sku: parsed.sku,
                    dp_code: parsed.dp_code,
                    asin: parsed.asin.clone(),
                    amazon_url: config::product_url(&parsed.asin),
                    price_text: None,
                    price_value: None,
                    currency: "JPY".to_string(),
                    status: RowStatus::Pending,
                    error: None,
                    fetched_at: None,
                });
            }
            Err(SkuError::Empty) => {}
            Err(SkuError::InvalidFormat(value)) => {
                rows.push(crate::models::empty_row(
                    trimmed.to_string(),
                    value.clone(),
                    value,
                    RowStatus::FormatError,
                    Some("SKU 格式无效，ASIN 需为 10 位字母数字".to_string()),
                ));
            }
        }
    }

    (rows, duplicate_count)
}

pub fn parse_skus_from_file(path: &str) -> anyhow::Result<(Vec<RowResult>, usize)> {
    let content = fs::read_to_string(Path::new(path))?;
    Ok(parse_skus_from_text(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RowStatus;

    #[test]
    fn parses_normal_sku() {
        let parsed = parse_sku_line("gx-b0dfxqwpps149").unwrap();
        assert_eq!(parsed.dp_code, "b0dfxqwpps");
        assert_eq!(parsed.asin, "B0DFXQWPPS");
    }

    #[test]
    fn parses_without_prefix() {
        let parsed = parse_sku_line("b0dfxqwpps149").unwrap();
        assert_eq!(parsed.asin, "B0DFXQWPPS");
    }

    #[test]
    fn rejects_invalid_format() {
        assert!(parse_sku_line("gx-ab").is_err());
    }

    #[test]
    fn parses_bare_asin() {
        let parsed = parse_sku_line("b0dfxqwpps").unwrap();
        assert_eq!(parsed.dp_code, "b0dfxqwpps");
        assert_eq!(parsed.asin, "B0DFXQWPPS");
    }

    #[test]
    fn parses_bare_asin_ending_with_digits() {
        let parsed = parse_sku_line("4873115655").unwrap();
        assert_eq!(parsed.dp_code, "4873115655");
        assert_eq!(parsed.asin, "4873115655");
    }

    #[test]
    fn parses_uppercase_prefix() {
        let parsed = parse_sku_line("GX-B018AOIO1Y150").unwrap();
        assert_eq!(parsed.dp_code, "B018AOIO1Y");
        assert_eq!(parsed.asin, "B018AOIO1Y");
    }

    #[test]
    fn rejects_13_char_non_digit_suffix() {
        assert!(parse_sku_line("b0dfxqwppsabc").is_err());
    }

    #[test]
    fn rejects_invalid_lengths() {
        assert!(parse_sku_line("b0dfxqwpps1").is_err());
        assert!(parse_sku_line("b0dfxqwp").is_err());
    }

    #[test]
    fn deduplicates_bare_asin_with_suffixed_sku() {
        let text = "gx-b0dfxqwpps149\nb0dfxqwpps\n";
        let (rows, dup) = parse_skus_from_text(text);
        assert_eq!(dup, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].asin, "B0DFXQWPPS");
    }

    #[test]
    fn deduplicates_and_marks_invalid() {
        let text = "gx-b0dfxqwpps149\ngx-b0dfxqwpps149\nbad\n";
        let (rows, dup) = parse_skus_from_text(text);
        assert_eq!(dup, 1);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].asin, "B0DFXQWPPS");
        assert_eq!(rows[1].status, RowStatus::FormatError);
    }

    #[test]
    fn parses_ids_file() {
        let content = std::fs::read_to_string(format!(
            "{}/../ids.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_default();
        if content.is_empty() {
            return;
        }
        let (rows, _) = parse_skus_from_text(&content);
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().all(|r| r.status == RowStatus::Pending));
    }
}
