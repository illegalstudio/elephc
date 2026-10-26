//! Purpose:
//! Exposes fixture phase boundaries when remote CI terminates a test before it returns.
//!
//! Called from:
//! - Compiler and runner helpers, with timing enabled explicitly by the test environment.
//!
//! Key details:
//! - ELEPHC_FIXTURE_PHASE_TIMING accepts 1 or comma-separated test-name suffixes.
//! - Writes to the harness descriptor directly so timeout kills cannot discard libtest buffers.
//! - Compiled-program output and watchdog limits are unaffected.

use std::time::Instant;

/// Records one enabled harness phase until its guard leaves scope.
pub(crate) struct FixturePhaseTiming {
    phase: &'static str,
    fixture: String,
    started: Instant,
}

/// Starts a phase only for explicitly selected libtest thread names, defaulting to silence.
pub(crate) fn fixture_phase_timing(phase: &'static str) -> Option<FixturePhaseTiming> {
    let filter = std::env::var("ELEPHC_FIXTURE_PHASE_TIMING").ok()?;
    let thread = std::thread::current();
    let fixture = thread.name().unwrap_or("unnamed");
    if filter != "1" && !filter.split(',').map(str::trim).any(|suffix| !suffix.is_empty() && fixture.ends_with(suffix)) {
        return None;
    }
    let span = FixturePhaseTiming { phase, fixture: fixture.into(), started: Instant::now() };
    write_phase_line(&format!("fixture-phase {} {} start\n", span.fixture, phase));
    Some(span)
}

impl Drop for FixturePhaseTiming {
    /// Publishes completion without changing the fixture's assertion or unwind behavior.
    fn drop(&mut self) {
        write_phase_line(&format!("fixture-phase {} {} end elapsed_ms={}\n", self.fixture, self.phase, self.started.elapsed().as_millis()));
    }
}

/// Emits one best-effort newline-terminated record outside libtest's buffered print capture.
fn write_phase_line(line: &str) {
    // SAFETY: the borrowed UTF-8 bytes stay valid for this synchronous descriptor write.
    let _ = unsafe { libc::write(2, line.as_ptr().cast(), line.len()) };
}
