//! Purpose:
//! Owns terminal progress/decoration state for the CLI: a live per-phase spinner
//! and the single "decorated vs. plain" switch that also gates error/warning and
//! `--timings` styling elsewhere in the compiler.
//!
//! Called from:
//! - `crate::pipeline::compile()` at every phase boundary and terminal
//!   success/failure point.
//! - `crate::errors::report` and `crate::timings` read `is_decorated()` to match
//!   the same on/off switch.
//!
//! Key details:
//! - State is process-global (`OnceLock`), set exactly once by `init()` at the
//!   very start of `compile()`, mirroring the existing `codegen::set_null_repr`/
//!   `strict_php::set_enabled` global-setup pattern in this codebase.

use std::sync::OnceLock;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

static DECORATED: OnceLock<bool> = OnceLock::new();
static BAR: OnceLock<Option<ProgressBar>> = OnceLock::new();

/// Decides whether this run gets spinner/color/event decoration: never when
/// `--quiet` was passed, otherwise only when stderr is an interactive terminal.
fn compute_decorated(quiet: bool, stderr_is_term: bool) -> bool {
    !quiet && stderr_is_term
}

/// Initializes the global progress/decoration state. Must be called exactly
/// once, before any other function in this module, from `pipeline::compile()`.
pub(crate) fn init(quiet: bool) {
    let decorated = compute_decorated(quiet, console::Term::stderr().is_term());
    let _ = DECORATED.set(decorated);
    let bar = if decorated {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .expect("static spinner template is valid"),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };
    let _ = BAR.set(bar);
}

/// Whether this run is decorated (spinner/colors/symbols/event lines). Read by
/// `errors::report` and `timings::CompileTimings::report` to match this switch.
pub(crate) fn is_decorated() -> bool {
    *DECORATED.get().unwrap_or(&false)
}

fn bar() -> Option<&'static ProgressBar> {
    BAR.get().and_then(|b| b.as_ref())
}

/// Updates the live spinner's message to the current compile phase name.
/// No-op when not decorated.
pub(crate) fn phase(name: &str) {
    if let Some(pb) = bar() {
        pb.set_message(name.to_string());
    }
}

/// Stops and clears the live spinner, e.g. before a fatal error is reported or
/// before a final success line is printed, so neither interleaves with a
/// mid-animation spinner frame. No-op when not decorated.
pub(crate) fn clear() {
    if let Some(pb) = bar() {
        pb.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `--quiet` forces non-decorated output even on a real terminal.
    #[test]
    fn quiet_forces_non_decorated_regardless_of_terminal() {
        assert!(!compute_decorated(true, true));
        assert!(!compute_decorated(true, false));
    }

    /// Verifies decoration follows terminal detection when `--quiet` is absent.
    #[test]
    fn non_quiet_follows_terminal_detection() {
        assert!(compute_decorated(false, true));
        assert!(!compute_decorated(false, false));
    }
}
