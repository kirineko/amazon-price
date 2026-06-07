use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "sid";

struct LoginLimiter {
    failures: VecDeque<Instant>,
    max_attempts: usize,
    window: Duration,
}

impl LoginLimiter {
    fn new() -> Self {
        Self {
            failures: VecDeque::new(),
            max_attempts: 5,
            window: Duration::from_secs(300),
        }
    }

    fn is_blocked(&mut self) -> bool {
        let now = Instant::now();
        while self
            .failures
            .front()
            .is_some_and(|t| now.duration_since(*t) > self.window)
        {
            self.failures.pop_front();
        }
        self.failures.len() >= self.max_attempts
    }

    fn record_failure(&mut self) {
        self.failures.push_back(Instant::now());
    }

    fn reset(&mut self) {
        self.failures.clear();
    }
}

pub struct AuthState {
    password_hash: String,
    sessions: Mutex<HashSet<String>>,
    secure_cookies: bool,
    login_limiter: Mutex<LoginLimiter>,
}

impl AuthState {
    pub fn new(password_hash: &str, secure_cookies: bool) -> Arc<Self> {
        Arc::new(Self {
            password_hash: password_hash.to_string(),
            sessions: Mutex::new(HashSet::new()),
            secure_cookies,
            login_limiter: Mutex::new(LoginLimiter::new()),
        })
    }

    pub fn verify_password(&self, password: &str) -> bool {
        let parsed = match PasswordHash::new(&self.password_hash) {
            Ok(h) => h,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }

    pub fn create_session(&self) -> String {
        let id = Uuid::new_v4().to_string();
        self.sessions.lock().insert(id.clone());
        id
    }

    pub fn destroy_session(&self, sid: &str) {
        self.sessions.lock().remove(sid);
    }

    pub fn is_valid_session(&self, sid: &str) -> bool {
        self.sessions.lock().contains(sid)
    }

    pub fn secure_cookies(&self) -> bool {
        self.secure_cookies
    }

    pub fn login_blocked(&self) -> bool {
        self.login_limiter.lock().is_blocked()
    }

    pub fn record_login_failure(&self) {
        self.login_limiter.lock().record_failure();
    }

    pub fn reset_login_failures(&self) {
        self.login_limiter.lock().reset();
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
}

pub fn session_from_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub fn set_session_cookie(sid: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={sid}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn clear_session_cookie(secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub async fn auth_middleware(
    State(auth): State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // 挂载在 /api nest 下，路径形如 /login、/session（无 /api 前缀）
    if path == "/login" {
        return next.run(request).await;
    }

    let sid = session_from_cookie(request.headers());
    let valid = sid
        .as_deref()
        .map(|s| auth.is_valid_session(s))
        .unwrap_or(false);

    if valid {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "未认证，请先登录" })),
        )
            .into_response()
    }
}
