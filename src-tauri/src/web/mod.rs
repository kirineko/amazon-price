pub mod auth;
pub mod handlers;
pub mod routes;

use crate::models::ScrapeProgress;
use crate::state::AppState;
use auth::AuthState;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct WebConfig {
    pub password_hash: String,
    pub static_dir: PathBuf,
    pub port: u16,
    pub secure_cookies: bool,
}

pub struct WebState {
    pub app: Arc<AppState>,
    pub auth: Arc<AuthState>,
    pub progress_tx: broadcast::Sender<ScrapeProgress>,
}

pub async fn serve(config: WebConfig) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let (progress_tx, _) = broadcast::channel(256);
    let web_state = Arc::new(WebState {
        app: Arc::new(AppState::new()),
        auth: AuthState::new(&config.password_hash, config.secure_cookies),
        progress_tx,
    });

    let app = routes::router(web_state, &config);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn load_config() -> anyhow::Result<WebConfig> {
    let password_hash = std::env::var("APP_PASSWORD_HASH")
        .map_err(|_| anyhow::anyhow!("缺少环境变量 APP_PASSWORD_HASH"))?;
    if password_hash.is_empty() {
        anyhow::bail!("APP_PASSWORD_HASH 不能为空");
    }

    let static_dir = std::env::var("STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("dist"));

    if !static_dir.exists() {
        tracing::warn!("静态目录不存在: {}", static_dir.display());
    }

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let secure_cookies = std::env::var("SECURE_COOKIES")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    Ok(WebConfig {
        password_hash,
        static_dir,
        port,
        secure_cookies,
    })
}

pub fn progress_callback(
    tx: broadcast::Sender<ScrapeProgress>,
) -> crate::scraper::ProgressCallback {
    Arc::new(move |progress| {
        let _ = tx.send(progress);
    })
}
