//! Cancellation token shared across the runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    inner: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn request(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }

    pub fn requested(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }

    pub fn throw_if_requested(&self) -> Result<(), Cancelled> {
        if self.requested() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cancellation requested")
    }
}

impl std::error::Error for Cancelled {}

/// Install Ctrl+C handler that requests graceful cancellation.
pub fn install_ctrlc(token: CancelToken) -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        token.request();
    })
}
