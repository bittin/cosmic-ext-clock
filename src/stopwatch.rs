// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct Stopwatch {
    elapsed: Duration,
    started_at: Option<Instant>,
    laps: Vec<Duration>,
}

impl Stopwatch {
    pub fn start(&mut self, now: Instant) {
        if self.started_at.is_none() {
            self.started_at = Some(now);
        }
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(started) = self.started_at.take() {
            self.elapsed += now.saturating_duration_since(started);
        }
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.started_at = None;
        self.laps.clear();
    }

    pub fn lap(&mut self, now: Instant) {
        if self.is_running() {
            self.laps.push(self.elapsed_at(now));
        }
    }

    #[must_use]
    pub fn elapsed_at(&self, now: Instant) -> Duration {
        self.elapsed
            + self.started_at.map_or(Duration::ZERO, |started| {
                now.saturating_duration_since(started)
            })
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    #[must_use]
    pub fn laps(&self) -> &[Duration] {
        &self.laps
    }
}
