//! Purpose:
//! Models monotonic, non-overlapping periodic admission independently of PHP and poll transport.
//!
//! Called from:
//! - `super` native registration operations and deterministic cadence tests.
//!
//! Key details:
//! - First admission is delayed; missed deadlines are skipped without catch-up bursts.
//! - An idle interval change rearms from the command; an active change rearms from completion.
//! - Stop is irreversible, and no second invocation is admitted while one is active.

use std::time::{Duration, Instant};

pub(super) struct Cadence {
    interval: Duration,
    deadline: Instant,
    running: bool,
    stopped: bool,
    changed_while_running: bool,
}

/// Validates the seconds surface against nanosecond precision and signed timer range.
pub(super) fn interval(seconds: f64) -> Option<Duration> {
    let duration = Duration::try_from_secs_f64(seconds).ok()?;
    if duration.is_zero() || duration.as_nanos() > i64::MAX as u128 { return None; }
    Some(duration)
}

impl Cadence {
    /// Creates the first future deadline without admitting work synchronously.
    pub(super) fn new(interval: Duration, now: Instant) -> Option<Self> {
        Some(Self { interval, deadline: now.checked_add(interval)?, running: false,
            stopped: false, changed_while_running: false })
    }

    /// Returns seconds until admission, or minus one after stop or during an active invocation.
    pub(super) fn remaining(&self, now: Instant) -> f64 {
        if self.stopped || self.running { return -1.0; }
        self.deadline.saturating_duration_since(now).as_secs_f64()
    }

    /// Admits at most one invocation; zero waits, one admits, two denotes stopped/busy.
    pub(super) fn claim(&mut self, now: Instant) -> i64 {
        if self.stopped || self.running { return 2; }
        if now < self.deadline { return 0; }
        self.running = true;
        1
    }

    /// Applies a validated interval change without disturbing an active invocation.
    pub(super) fn set_interval(&mut self, interval: Duration, now: Instant) -> bool {
        if self.stopped { return false; }
        if interval == self.interval { return true; }
        let Some(deadline) = now.checked_add(interval) else { return false; };
        self.interval = interval;
        if self.running { self.changed_while_running = true; }
        else { self.deadline = deadline; }
        true
    }

    /// Permanently closes admission while allowing an already admitted invocation to settle.
    pub(super) fn stop(&mut self) { self.stopped = true; }

    /// Reports whether retirement must wait for the admitted invocation's settlement.
    pub(super) fn running(&self) -> bool { self.running }

    /// Settles one invocation and computes the next strictly future fixed-cadence deadline.
    pub(super) fn complete(&mut self, now: Instant) -> bool {
        if !self.running { return false; }
        self.running = false;
        if self.stopped { return true; }
        let delay = if self.changed_while_running {
            self.changed_while_running = false;
            self.interval
        } else {
            let elapsed = now.saturating_duration_since(self.deadline).as_nanos();
            let interval = self.interval.as_nanos();
            Duration::from_nanos((interval - elapsed % interval) as u64)
        };
        let Some(deadline) = now.checked_add(delay) else { self.stopped = true; return false; };
        self.deadline = deadline;
        true
    }
}
