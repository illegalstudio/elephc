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

    /// Returns elapsed time since this collector was constructed, regardless of
    /// whether timing collection is enabled. Used for the final success line's
    /// elapsed-seconds suffix even when `--timings` was not passed.
    pub(crate) fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Prints the collected timing report to stderr.
    ///
    /// Output is gated behind the `enabled` flag. The report includes all notes
    /// in insertion order, each phase with its duration in milliseconds, and a
    /// total elapsed time from when the collector was constructed.
    /// Prints the collected timing report to stderr.
    ///
    /// Output is gated behind the `enabled` flag. The report includes all notes
    /// in insertion order, each phase with its duration in milliseconds and share
    /// of the total, and a total elapsed time from when the collector was
    /// constructed. Decorated runs (see `crate::progress::is_decorated`) bold the
    /// header and total row; plain runs keep today's unstyled table.
    pub(crate) fn report(&self) {
        if !self.enabled {
            return;
        }

        let decorated = crate::progress::is_decorated();
        let total = self.started_at.elapsed();

        eprintln!("{}", style_if(decorated, "Compiler timings:"));
        for note in &self.notes {
            eprintln!("  {}", note);
        }
        for (phase, duration) in &self.phases {
            eprintln!(
                "  {:<12} {:>8.2} ms {:>5.1}%",
                phase,
                duration.as_secs_f64() * 1000.0,
                percentage(*duration, total),
            );
        }
        let total_line = format!("  {:<12} {:>8.2} ms", "total", total.as_secs_f64() * 1000.0);
        eprintln!("{}", style_if(decorated, &total_line));
    }
}

/// Returns `part`'s share of `total` as a percentage, or `0.0` when `total` is
/// zero (guards the first phase recorded before any measurable time elapses).
fn percentage(part: Duration, total: Duration) -> f64 {
    if total.as_secs_f64() == 0.0 {
        0.0
    } else {
        part.as_secs_f64() / total.as_secs_f64() * 100.0
    }
}

fn style_if(decorated: bool, text: &str) -> String {
    if decorated {
        console::style(text).bold().to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_of_zero_total_is_zero() {
        assert_eq!(percentage(Duration::from_millis(5), Duration::ZERO), 0.0);
    }

    #[test]
    fn percentage_computes_share_of_total() {
        let pct = percentage(Duration::from_millis(25), Duration::from_millis(100));
        assert!((pct - 25.0).abs() < 0.001);
    }

    #[test]
    fn style_if_plain_is_unchanged() {
        assert_eq!(style_if(false, "Compiler timings:"), "Compiler timings:");
    }

    #[test]
    fn style_if_decorated_contains_original_text() {
        assert!(style_if(true, "Compiler timings:").contains("Compiler timings:"));
    }
}
