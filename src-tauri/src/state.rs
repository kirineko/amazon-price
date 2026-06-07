use crate::models::{ProxyConfig, RowResult};
use crate::region::AmazonSession;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct AppState {
    pub session: Mutex<Option<AmazonSession>>,
    pub cancel_flag: Arc<AtomicBool>,
    pub last_rows: Mutex<Vec<RowResult>>,
    pub proxy: Mutex<ProxyConfig>,
    pub config_dir: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            last_rows: Mutex::new(Vec::new()),
            proxy: Mutex::new(ProxyConfig::default()),
            config_dir: Mutex::new(None),
        }
    }

    pub fn set_config_dir(&self, dir: PathBuf) {
        *self.config_dir.lock() = Some(dir);
    }

    pub fn config_dir(&self) -> Option<PathBuf> {
        self.config_dir.lock().clone()
    }

    pub fn proxy_config(&self) -> ProxyConfig {
        self.proxy.lock().clone()
    }

    pub fn set_proxy_config(&self, config: ProxyConfig) {
        *self.proxy.lock() = config;
    }

    pub fn clear_session(&self) {
        *self.session.lock() = None;
    }
}
