// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TimerId(u64);

impl TimerId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerStatus {
    Ready,
    Running,
    Paused,
    Finished,
}

#[derive(Clone, Debug)]
pub struct Timer {
    id: TimerId,
    pub label: String,
    duration: Duration,
    elapsed: Duration,
    started_at: Option<Instant>,
    status: TimerStatus,
    ringing: bool,
}

impl Timer {
    #[must_use]
    pub fn new(id: TimerId, label: impl Into<String>, duration: Duration) -> Option<Self> {
        (!duration.is_zero()).then(|| Self {
            id,
            label: label.into(),
            duration,
            elapsed: Duration::ZERO,
            started_at: None,
            status: TimerStatus::Ready,
            ringing: false,
        })
    }

    #[must_use]
    pub const fn id(&self) -> TimerId {
        self.id
    }

    pub fn start(&mut self, now: Instant) {
        if matches!(self.status, TimerStatus::Ready | TimerStatus::Paused) {
            self.started_at = Some(now);
            self.status = TimerStatus::Running;
        }
    }

    pub fn pause(&mut self, now: Instant) {
        if self.status == TimerStatus::Running {
            self.elapsed = self.elapsed_at(now).min(self.duration);
            self.started_at = None;
            self.status = if self.elapsed >= self.duration {
                TimerStatus::Finished
            } else {
                TimerStatus::Paused
            };
        }
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.started_at = None;
        self.status = TimerStatus::Ready;
        self.ringing = false;
    }

    pub fn reconfigure(&mut self, label: impl Into<String>, duration: Duration) -> bool {
        if duration.is_zero() {
            return false;
        }
        self.label = label.into();
        self.duration = duration;
        self.reset();
        true
    }

    #[must_use]
    pub fn status(&self) -> TimerStatus {
        self.status
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    #[must_use]
    pub fn remaining_at(&self, now: Instant) -> Duration {
        self.duration.saturating_sub(self.elapsed_at(now))
    }

    pub fn update(&mut self, now: Instant) -> bool {
        if self.status == TimerStatus::Running && self.remaining_at(now).is_zero() {
            self.elapsed = self.duration;
            self.started_at = None;
            self.status = TimerStatus::Finished;
            self.ringing = true;
            return true;
        }
        false
    }

    #[must_use]
    pub const fn is_ringing(&self) -> bool {
        self.ringing
    }

    pub fn acknowledge(&mut self) -> bool {
        if !self.ringing {
            return false;
        }
        self.ringing = false;
        true
    }

    fn elapsed_at(&self, now: Instant) -> Duration {
        self.elapsed
            + self.started_at.map_or(Duration::ZERO, |started| {
                now.saturating_duration_since(started)
            })
    }
}
