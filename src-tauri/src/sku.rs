use crate::config;
use crate::models::{ParseSkusResult, RowResult, RowStatus};
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

    let body = strip_gx_prefix(trimmed);
    let dp_code = derive_dp_code(&body);
    let asin = dp_code.to_uppercase();

    if !is_valid_asin(&asin) {
        return Err(SkuError::InvalidFormat(trimmed.to_string()));
    }

    Ok((dp_code, asin))
}

fn strip_gx_prefix(raw: &str) -> String {
    let mut body = raw.to_string();
    if body.len() >= 3 && body[..3].eq_ignore_ascii_case("gx-") {
        body = body[3..].to_string();
    }
    body
}

fn derive_dp_code(body: &str) -> String {
    if body.len() == 13 && body[10..].chars().all(|c| c.is_ascii_digit()) {
        body[..10].to_string()
    } else {
        body.to_string()
    }
}

pub fn sku_format_error_message(raw: &str) -> String {
    let body = strip_gx_prefix(raw.trim());
    let len = body.chars().count();
    if len == 10 {
        "应为 10 位 ASIN（仅含字母数字）".to_string()
    } else if len == 13 && body.chars().skip(10).all(|c| c.is_ascii_digit()) {
        "13 位格式中前 10 位须为有效 ASIN（仅含字母数字）".to_string()
    } else {
        format!(
            "去掉可选 gx- 后为 {len} 位，应为 10 位 ASIN，或 13 位且末 3 位为数字后缀"
        )
    }
}

pub fn is_valid_asin(asin: &str) -> bool {
    asin.len() == 10 && asin.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn parse_sku_line(line: &str) -> Result<ParsedSku, SkuError> {
    let sku = line.trim().to_string();
    let (dp_code, asin) = parse_dp_code(&sku)?;
    Ok(ParsedSku { sku, dp_code, asin })
}

pub fn parse_skus_from_text(text: &str) -> ParseSkusResult {
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
                    amazon_url: config::search_url(&parsed.asin),
                    price_text: None,
                    price_value: None,
                    currency: "JPY".to_string(),
                    status: RowStatus::Pending,
                    error: None,
                    fetched_at: None,
                });
            }
            Err(SkuError::Empty) => {}
            Err(SkuError::InvalidFormat(raw)) => {
                rows.push(crate::models::empty_row(
                    trimmed.to_string(),
                    raw.clone(),
                    raw.to_uppercase(),
                    RowStatus::FormatError,
                    Some(sku_format_error_message(trimmed)),
                ));
            }
        }
    }

    let invalid_count = rows
        .iter()
        .filter(|row| row.status == RowStatus::FormatError)
        .count();
    let valid_count = rows.len() - invalid_count;

    ParseSkusResult {
        rows,
        duplicate_count,
        invalid_count,
        valid_count,
    }
}

pub fn parse_skus_from_file(path: &str) -> anyhow::Result<ParseSkusResult> {
    let content = fs::read_to_string(Path::new(path))?;
    Ok(parse_skus_from_text(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RowStatus;

    #[test]
    fn parsed_row_uses_search_page_url() {
        let result = parse_skus_from_text("gx-b01buq774e155\n");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0].amazon_url,
            "https://www.amazon.co.jp/s?k=b01buq774e"
        );
    }

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
    fn parses_gx_prefix_optional_with_and_without_suffix() {
        assert_eq!(
            parse_sku_line("gx-b08ckgrhlf").unwrap().asin,
            "B08CKGRHLF"
        );
        assert_eq!(
            parse_sku_line("b08ckgrhlf149").unwrap().asin,
            "B08CKGRHLF"
        );
        assert_eq!(parse_sku_line("B08CKGRHLF").unwrap().asin, "B08CKGRHLF");
    }

    #[test]
    fn format_error_explains_length_rules() {
        let msg = sku_format_error_message("gx-ab");
        assert!(msg.contains("10 位"));
        let msg = sku_format_error_message("b0dfxqwpps1");
        assert!(msg.contains("11 位"));
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
        let result = parse_skus_from_text(text);
        assert_eq!(result.duplicate_count, 1);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].asin, "B0DFXQWPPS");
        assert_eq!(result.valid_count, 1);
        assert_eq!(result.invalid_count, 0);
    }

    #[test]
    fn deduplicates_and_marks_invalid() {
        let text = "gx-b0dfxqwpps149\ngx-b0dfxqwpps149\nbad\n";
        let result = parse_skus_from_text(text);
        assert_eq!(result.duplicate_count, 1);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].asin, "B0DFXQWPPS");
        assert_eq!(result.rows[1].status, RowStatus::FormatError);
        assert_eq!(result.valid_count, 1);
        assert_eq!(result.invalid_count, 1);
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
        let result = parse_skus_from_text(&content);
        assert_eq!(result.rows.len(), 6);
        assert_eq!(result.valid_count, 6);
        assert!(result.rows.iter().all(|r| r.status == RowStatus::Pending));
    }
}
