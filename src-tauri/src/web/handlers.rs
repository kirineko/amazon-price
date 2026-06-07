use crate::models::{RowResult, ScrapeOptions};
use crate::service;
use crate::web::auth::{self, LoginRequest};
use crate::web::{progress_callback, WebState};
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn err_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

pub async fn login(
    State(web): State<Arc<WebState>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    if web.auth.login_blocked() {
        return err_response(StatusCode::TOO_MANY_REQUESTS, "登录尝试过多，请稍后再试");
    }

    if !web.auth.verify_password(&body.password) {
        web.auth.record_login_failure();
        return err_response(StatusCode::UNAUTHORIZED, "认证失败");
    }

    web.auth.reset_login_failures();
    let sid = web.auth.create_session();
    let cookie = auth::set_session_cookie(&sid, web.auth.secure_cookies());

    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

pub async fn logout(State(web): State<Arc<WebState>>, headers: HeaderMap) -> Response {
    if let Some(sid) = auth::session_from_cookie(&headers) {
        web.auth.destroy_session(&sid);
    }
    let cookie = auth::clear_session_cookie(web.auth.secure_cookies());
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

pub async fn auth_status(headers: HeaderMap, State(web): State<Arc<WebState>>) -> Response {
    let authenticated = auth::session_from_cookie(&headers)
        .map(|sid| web.auth.is_valid_session(&sid))
        .unwrap_or(false);
    Json(auth::AuthStatus { authenticated }).into_response()
}

pub async fn init_session(
    State(web): State<Arc<WebState>>,
    Json(body): Json<InitSessionRequest>,
) -> Response {
    match service::init_session(&web.app, body.zip_code).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
    }
}

#[derive(Deserialize)]
pub struct InitSessionRequest {
    #[serde(rename = "zipCode")]
    pub zip_code: Option<String>,
}

#[derive(Deserialize)]
pub struct ParseSkusRequest {
    pub text: String,
}

pub async fn parse_skus(
    Json(body): Json<ParseSkusRequest>,
) -> impl IntoResponse {
    let (rows, duplicate_count) = service::parse_skus(&body.text);
    Json(serde_json::json!({ "rows": rows, "duplicateCount": duplicate_count }))
}

#[derive(Deserialize)]
pub struct ScrapeRequest {
    pub rows: Vec<RowResult>,
    pub options: Option<ScrapeOptions>,
}

pub async fn start_scrape(
    State(web): State<Arc<WebState>>,
    Json(body): Json<ScrapeRequest>,
) -> Response {
    let on_progress = Some(progress_callback(web.progress_tx.clone()));
    match service::start_scrape(&web.app, body.rows, body.options, on_progress).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
    }
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub row: Option<RowResult>,
    pub options: Option<ScrapeOptions>,
}

pub async fn refresh(
    State(web): State<Arc<WebState>>,
    Json(body): Json<RefreshRequest>,
) -> Response {
    let on_progress = Some(progress_callback(web.progress_tx.clone()));
    let result = if let Some(row) = body.row {
        service::refresh_one(&web.app, row, body.options, on_progress)
            .await
            .map(|row| vec![row])
    } else {
        service::refresh_all(&web.app, body.options, on_progress).await
    };

    match result {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => err_response(StatusCode::BAD_REQUEST, e),
    }
}

pub async fn cancel_scrape(State(web): State<Arc<WebState>>) -> impl IntoResponse {
    service::cancel_scrape(&web.app);
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct ExportCsvRequest {
    pub rows: Vec<RowResult>,
}

pub async fn export_csv(Json(body): Json<ExportCsvRequest>) -> impl IntoResponse {
    let csv = service::export_csv(&body.rows);
    (
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
        csv,
    )
}

pub async fn self_check(
    State(web): State<Arc<WebState>>,
    axum::extract::Query(query): axum::extract::Query<InitSessionRequest>,
) -> Response {
    match service::run_self_check(&web.app, query.zip_code).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
    }
}

pub fn sse_no_buffer_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Accel-Buffering", "no".parse().unwrap());
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    headers
}

pub async fn events_with_headers(State(web): State<Arc<WebState>>) -> Response {
    let mut rx = web.progress_tx.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(progress) => {
                    if let Ok(json) = serde_json::to_string(&progress) {
                        yield Ok::<Event, Infallible>(Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let sse = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)));

    let mut response = sse.into_response();
    response.headers_mut().extend(sse_no_buffer_headers());
    response
}
