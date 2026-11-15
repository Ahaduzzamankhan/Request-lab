//! Timing helpers for request measurement.

use std::time::{Duration, Instant};

/// Measures how long a request takes from send to a fully read body.
#[derive(Debug)]
pub struct RequestTimer {
    started: Instant,
    stopped_ms: Option<u64>,
}

impl RequestTimer {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
            stopped_ms: None,
        }
    }

    pub fn stop(&mut self) -> u64 {
        let elapsed = self.elapsed_ms();
        self.stopped_ms = Some(elapsed);
        elapsed
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// The instant this timer started, for response capture.
    pub fn started_at(&self) -> Instant {
        self.started
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.stopped_ms.unwrap_or_else(|| {
            let millis = self.started.elapsed().as_millis();
            millis.min(u64::MAX as u128) as u64
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_measures_and_freezes() {
        let mut timer = RequestTimer::start();
        std::thread::sleep(Duration::from_millis(12));
        assert!(timer.elapsed_ms() >= 10);
        let stopped = timer.stop();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(timer.elapsed_ms(), stopped);
        assert!(timer.elapsed() >= Duration::from_millis(10));
    }
}
