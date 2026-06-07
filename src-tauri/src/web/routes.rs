use crate::web::auth::auth_middleware;
use crate::web::handlers;
use crate::web::{WebConfig, WebState};
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

pub fn router(web_state: Arc<WebState>, config: &WebConfig) -> Router {
    let auth = web_state.auth.clone();

    let api = Router::new()
        .route("/login", post(handlers::login))
        .route("/logout", post(handlers::logout))
        .route("/auth/status", get(handlers::auth_status))
        .route("/session", post(handlers::init_session))
        .route("/skus/parse", post(handlers::parse_skus))
        .route("/scrape", post(handlers::start_scrape))
        .route("/scrape/refresh", post(handlers::refresh))
        .route("/scrape/cancel", post(handlers::cancel_scrape))
        .route("/export.csv", post(handlers::export_csv))
        .route("/self-check", get(handlers::self_check))
        .route("/events", get(handlers::events_with_headers))
        .layer(middleware::from_fn_with_state(auth, auth_middleware))
        .with_state(web_state);

    let index_path = config.static_dir.join("index.html");
    let static_service = ServeDir::new(&config.static_dir)
        .not_found_service(ServeFile::new(index_path));

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
