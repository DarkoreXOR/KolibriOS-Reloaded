//! Cancellation-aware timer.

use super::cancel::{CancelToken, Cancelled};
use std::thread;
use std::time::{Duration, Instant};

pub fn sleep_ms(ms: u64, cancel: &CancelToken, poll_ms: u64) -> Result<(), Cancelled> {
    let deadline = Instant::now() + Duration::from_millis(ms);
    let step = Duration::from_millis(poll_ms.max(1));
    while Instant::now() < deadline {
        cancel.throw_if_requested()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(step));
    }
    cancel.throw_if_requested()?;
    Ok(())
}
