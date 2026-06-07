mod commands;
pub mod config;
pub mod models;
pub mod proxy;
pub mod region;
pub mod scraper;
pub mod service;
pub mod sku;
pub mod state;

pub use state::AppState;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            let state = app.state::<AppState>();
            if let Ok(dir) = app.path().app_config_dir() {
                state.set_config_dir(dir.clone());
                if let Ok(loaded) = proxy::load_proxy_from_dir(&dir) {
                    state.set_proxy_config(loaded);
                }
            }
            Ok(())
        })
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
            commands::get_proxy,
            commands::set_proxy,
            commands::test_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod integration_tests {
    use super::models::ProxyConfig;
    use super::region::AmazonSession;
    use super::sku;

    #[tokio::test]
    #[ignore = "requires live access to amazon.co.jp"]
    async fn region_session_sets_japan_delivery() {
        let mut session = AmazonSession::new("150-0001", &ProxyConfig::default()).expect("session");
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
        let result = sku::parse_skus_from_text(&content);
        assert_eq!(result.rows.len(), 6);

        let mut session = AmazonSession::new("150-0001", &ProxyConfig::default()).expect("session");
        session.init_with_retry().await.expect("init session");

        let mut success = 0;
        for row in result.rows {
            let parsed = session.fetch_price(&row.asin).await.expect("fetch price");
            if parsed.price_text.is_some() {
                success += 1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        }

        assert_eq!(success, 6, "expected 6/6 successful prices");
    }

    #[test]
    fn default_request_interval_is_one_point_five_seconds() {
        assert_eq!(super::config::DEFAULT_REQUEST_INTERVAL_MS, 1500);
    }
}
