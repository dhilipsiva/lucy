//! Monotonic task time. Only a suspended host request stops this clock.
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(crate) struct ActiveClock {
    elapsed: Duration,
    running: Option<Instant>,
    budget_ms: u64,
}

impl ActiveClock {
    pub fn start(started: Instant, prior_ms: u64, budget_ms: u64) -> Self {
        Self {
            elapsed: Duration::from_millis(prior_ms),
            running: Some(started),
            budget_ms,
        }
    }

    pub fn resume(&mut self) {
        if self.running.is_none() {
            self.running = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        if let Some(started) = self.running.take() {
            self.elapsed += started.elapsed();
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        (self.elapsed + self.running.map_or(Duration::ZERO, |s| s.elapsed()))
            .as_millis()
            .min(u64::MAX as u128) as u64
    }

    pub fn remaining_ms(&self) -> u64 {
        self.budget_ms.saturating_sub(self.elapsed_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_counts_unrecorded_work_and_excludes_suspended_approval_waits() {
        let mut clock = ActiveClock::start(Instant::now() - Duration::from_millis(50), 10, 100);
        assert!(clock.elapsed_ms() >= 60);
        clock.pause();
        let paused = clock.elapsed_ms();
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(clock.elapsed_ms(), paused);
        clock.resume();
        clock.running = Some(Instant::now() - Duration::from_millis(100));
        assert_eq!(clock.remaining_ms(), 0);
        assert!(clock.elapsed_ms() >= 160);
    }
}
