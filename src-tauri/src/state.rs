use crate::models::RowResult;
use crate::region::AmazonSession;
use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct AppState {
    pub session: Mutex<Option<AmazonSession>>,
    pub cancel_flag: Arc<AtomicBool>,
    pub last_rows: Mutex<Vec<RowResult>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            last_rows: Mutex::new(Vec::new()),
        }
    }
}
