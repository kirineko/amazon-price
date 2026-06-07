#[cfg(feature = "desktop")]
mod commands;
pub mod config;
pub mod models;
pub mod region;
pub mod scraper;
pub mod service;
pub mod sku;
pub mod state;

#[cfg(feature = "web")]
pub mod web;

pub use state::AppState;

#[cfg(feature = "web")]
pub use web::WebConfig;

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::init_session,
            commands::parse_skus,
            commands::parse_skus_file,
            commands::start_scrape,
            commands::refresh_one,
            commands::refresh_all,
            commands::export_csv,
            commands::cancel_scrape,
            commands::run_self_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod integration_tests {
    use super::region::{parse_product_page, AmazonSession};
    use super::sku;

    #[tokio::test]
    #[ignore = "requires live access to amazon.co.jp"]
    async fn region_session_sets_japan_delivery() {
        let mut session = AmazonSession::new("150-0001").expect("session");
        session.init_with_retry().await.expect("init session");
        assert!(
            session
                .delivery_location
                .as_deref()
                .unwrap_or("")
                .contains("150-0001")
                || session
                    .delivery_location
                    .as_deref()
                    .unwrap_or("")
                    .contains("渋谷"),
            "delivery location should be Japan, got {:?}",
            session.delivery_location
        );
    }

    #[tokio::test]
    #[ignore = "requires live access to amazon.co.jp"]
    async fn scrape_ids_file_returns_prices() {
        let content = std::fs::read_to_string(format!(
            "{}/../ids.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("ids.txt");
        let (rows, _) = sku::parse_skus_from_text(&content);
        assert_eq!(rows.len(), 6);

        let mut session = AmazonSession::new("150-0001").expect("session");
        session.init_with_retry().await.expect("init session");

        let mut success = 0;
        for row in rows {
            let html = session
                .fetch_product_html(&row.asin)
                .await
                .expect("product html");
            let parsed = parse_product_page(&html, &row.asin);
            if parsed.price_text.is_some() {
                success += 1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        }

        assert_eq!(success, 6, "expected 6/6 successful prices");
    }

    #[test]
    fn rate_limit_defaults_are_within_spec() {
        assert!(super::config::DEFAULT_RATE_PER_SEC <= 3);
    }
}
