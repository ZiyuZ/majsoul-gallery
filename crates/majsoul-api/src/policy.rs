use crate::ApiError;
use std::{sync::Mutex, time::Duration};
use tokio::time::{Instant, sleep_until};

#[derive(Clone, Debug)]
pub struct Policy {
    pub request_interval: Duration,
    pub sync_interval: Duration,
    pub timeout: Duration,
    pub heartbeat_interval: Duration,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            request_interval: Duration::from_millis(1200),
            sync_interval: Duration::from_secs(60),
            timeout: Duration::from_secs(20),
            heartbeat_interval: Duration::from_secs(15),
        }
    }
}
pub(crate) struct Pacer {
    interval: Duration,
    next: Instant,
}
impl Pacer {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            next: Instant::now() + interval,
        }
    }
    pub async fn wait(&mut self) {
        sleep_until(self.next).await;
        self.next = Instant::now() + self.interval;
    }
}

/// Shared across clients so opening a new connection cannot bypass login cooldown.
pub(crate) fn reserve_login() -> Result<(), ApiError> {
    static LAST_LOGIN: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    let mut last = LAST_LOGIN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(previous) = *last {
        let remaining = Duration::from_secs(30).saturating_sub(previous.elapsed());
        if !remaining.is_zero() {
            return Err(ApiError::Cooldown {
                seconds: remaining.as_secs() + 1,
            });
        }
    }
    *last = Some(std::time::Instant::now());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn pacing_does_not_accumulate_a_burst_after_idle() {
        let mut pacer = Pacer::new(Duration::from_secs(2));
        pacer.wait().await;
        let start = Instant::now();
        pacer.wait().await;
        assert_eq!(start.elapsed(), Duration::from_secs(2));
        tokio::time::advance(Duration::from_secs(100)).await;
        pacer.wait().await;
        let start = Instant::now();
        pacer.wait().await;
        assert_eq!(start.elapsed(), Duration::from_secs(2));
    }
}
