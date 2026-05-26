//! Purpose:
//! Records optional compile-phase durations and notes for CLI timing output.
//! Keeps timing collection lightweight when the user has not requested `--timings`.
//!
//! Called from:
//! - `crate::pipeline::compile()` around each major compiler phase.
//!
//! Key details:
//! - Disabled timing still accepts calls so pipeline code does not branch around every measurement.

use std::time::{Duration, Instant};

/// Compile timing collector for optional performance profiling.
pub(crate) struct CompileTimings {
    enabled: bool,
    started_at: Instant,
    notes: Vec<String>,
    phases: Vec<(&'static str, Duration)>,
}

impl CompileTimings {
    /// Creates a new timing collector.
    ///
    /// `enabled` controls whether timing data is actually recorded and reported.
    /// When disabled (the common case), all methods are no-ops so callers need
    /// not branch on the flag. The internal timer starts immediately.
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            started_at: Instant::now(),
            notes: Vec::new(),
            phases: Vec::new(),
        }
    }

    /// Records the elapsed time since `started_at` for the named phase.
    ///
    /// No-op when timing collection is disabled. The duration is computed
    /// immediately at call time via `Instant::elapsed()`.
    pub(crate) fn record_since(&mut self, phase: &'static str, started_at: Instant) {
        if self.enabled {
            self.phases.push((phase, started_at.elapsed()));
        }
    }

    /// Appends an arbitrary informational note to the timing report.
    ///
    /// No-op when timing collection is disabled. The note is printed verbatim
    /// in order at the top of the report, before any phase timings.
    pub(crate) fn note(&mut self, note: impl Into<String>) {
        if self.enabled {
            self.notes.push(note.into());
        }
    }

    /// Prints the collected timing report to stderr.
    ///
    /// Output is gated behind the `enabled` flag. The report includes all notes
    /// in insertion order, each phase with its duration in milliseconds, and a
    /// total elapsed time from when the collector was constructed.
    pub(crate) fn report(&self) {
        if !self.enabled {
            return;
        }

        eprintln!("Compiler timings:");
        for note in &self.notes {
            eprintln!("  {}", note);
        }
        for (phase, duration) in &self.phases {
            eprintln!("  {:<12} {:>8.2} ms", phase, duration.as_secs_f64() * 1000.0);
        }
        eprintln!(
            "  {:<12} {:>8.2} ms",
            "total",
            self.started_at.elapsed().as_secs_f64() * 1000.0
        );
    }
}
