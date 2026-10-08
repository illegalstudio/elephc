//! Purpose:
//! Owns terminal progress/decoration state for the CLI: one live spinner per
//! compile phase, persistent completed-phase lines, and the single "decorated
//! vs. plain" switch used by diagnostics and `--timings`.
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
//! - A phase bar is finished and left in the terminal only after its matching
//!   `CompileTimings::record_since()` call reports successful completion.

#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

static DECORATED: OnceLock<bool> = OnceLock::new();
static BAR: OnceLock<Mutex<Option<ProgressBar>>> = OnceLock::new();

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
    let _ = BAR.set(Mutex::new(None));
}

/// Locks the active progress-bar slot, recovering its state after a poisoned lock.
fn bar_slot() -> MutexGuard<'static, Option<ProgressBar>> {
    BAR.get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Updates the live spinner's message to the current compile phase name.
/// No-op when not decorated.
pub(crate) fn phase(name: &str) {
    if !is_decorated() {
        return;
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("static spinner template is valid"),
    );
    pb.set_message(phase_label(name).to_string());
    pb.enable_steady_tick(Duration::from_millis(80));

    let mut slot = bar_slot();
    if let Some(previous) = slot.replace(pb) {
        previous.finish_and_clear();
    }
}

/// Replaces the active spinner with a persistent completed-phase line.
///
/// No-op in plain mode. A missing bar is tolerated so compiler errors and
/// nonstandard early-return paths never turn progress rendering into a
/// compilation failure.
pub(crate) fn finish_phase(name: &str, elapsed: Duration) {
    if !is_decorated() {
        return;
    }
    let Some(pb) = bar_slot().take() else {
        return;
    };
    pb.finish_and_clear();
    eprintln!("{}", format_completed_phase(name, elapsed));
}

/// Stops and clears the live spinner, e.g. before a fatal error is reported or
/// before a final success line is printed, so neither interleaves with a
/// mid-animation spinner frame. No-op when not decorated.
pub(crate) fn clear() {
    if let Some(pb) = bar_slot().take() {
        pb.finish_and_clear();
    }
}

/// Whether this run is decorated (spinner/colors/symbols/event lines). Read by
/// `errors::report` and `timings::CompileTimings::report` (Tasks 3/4) to match
/// this switch.
pub(crate) fn is_decorated() -> bool {
    *DECORATED.get().unwrap_or(&false)
}

/// Prints a cargo-style event line, e.g. a bridge library discovered by the
/// linker.
///
/// Uses the active progress bar when present so its animation stays intact,
/// otherwise writes directly to stderr between two completed phases. No-op in
/// plain mode.
pub(crate) fn event(verb: &str, detail: &str) {
    if !is_decorated() {
        return;
    }
    let line = format_event(verb, detail);
    if let Some(pb) = bar_slot().as_ref() {
        pb.println(line);
    } else {
        eprintln!("{line}");
    }
}

/// Clears the spinner (if any) and prints the final success line. Decorated
/// output adds a green checkmark and elapsed seconds; plain output is exactly
/// `message`, matching today's `println!("Compiled '{}' -> '{}'", ...)` text.
pub(crate) fn finish_ok(message: &str, elapsed: Duration) {
    clear();
    println!("{}", format_success(message, elapsed, is_decorated()));
}

/// Formats a cargo-style event line with an aligned, decorated verb.
fn format_event(verb: &str, detail: &str) -> String {
    format!("{:>12} {}", console::style(verb).bold().cyan(), detail)
}

/// Translates stable internal phase identifiers into action-oriented CLI text.
pub(crate) fn phase_label(name: &str) -> &str {
    match name {
        "read" => "Reading source",
        "tokenize" => "Tokenizing source",
        "parse" => "Parsing program",
        "magic-constants" => "Expanding magic constants",
        "autoload-build" => "Building autoload index",
        "resolve" => "Resolving includes",
        "pdo-prelude" => "Configuring PDO support",
        "mysqli-prelude" => "Configuring mysqli support",
        "tz-prelude" => "Configuring timezone support",
        "list-id-prelude" => "Loading timezone identifiers",
        "var-export-prelude" => "Configuring var_export()",
        "http-build-query-prelude" => "Configuring http_build_query()",
        "opcache-prelude" => "Configuring OPcache",
        "image-prelude" => "Configuring image support",
        "hash-prelude" => "Configuring hash support",
        "web-prelude" => "Configuring web runtime",
        "version-prelude" => "Applying PHP version profile",
        "name-resolve" => "Resolving names",
        "autoload-run" => "Discovering autoloaded symbols",
        "opcache-manifest-bake" => "Baking OPcache manifest",
        "opt-fold" => "Folding constants",
        "typecheck" => "Checking types",
        "exports-scan" => "Discovering exports",
        "opt-prop" => "Propagating constants",
        "opt-post" => "Pruning constant branches",
        "opt-norm" => "Normalizing control flow",
        "dce" => "Eliminating dead code",
        "decl-reach" => "Pruning unreachable declarations",
        "ir-lower" => "Lowering program to EIR",
        "ir-opt" => "Optimizing EIR",
        "ir-print" => "Rendering EIR",
        "runtime-cache" => "Preparing runtime object",
        "codegen" => "Generating native code",
        "write-asm" => "Writing assembly",
        "source-map" => "Writing source map",
        "assemble" => "Assembling object file",
        "link" => "Linking native output",
        _ => name,
    }
}

/// Formats one successful phase with an adaptive millisecond/second duration.
fn format_completed_phase(name: &str, elapsed: Duration) -> String {
    format!(
        "{} {} ({})",
        console::style("\u{2713}").bold().green(),
        phase_label(name),
        format_phase_duration(elapsed),
    )
}

/// Formats short phases in milliseconds and longer phases in seconds.
pub(crate) fn format_phase_duration(elapsed: Duration) -> String {
    if elapsed < Duration::from_secs(1) {
        format!("{:.2} ms", elapsed.as_secs_f64() * 1000.0)
    } else {
        format!("{:.2} s", elapsed.as_secs_f64())
    }
}

/// Formats the terminal success message, preserving the historical plain form.
fn format_success(message: &str, elapsed: Duration, decorated: bool) -> String {
    if decorated {
        format!(
            "{} {} ({:.2}s)",
            console::style("\u{2713}").bold().green(),
            message,
            elapsed.as_secs_f64()
        )
    } else {
        message.to_string()
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

    /// Verifies plain-mode success text is byte-identical to the historical format.
    #[test]
    fn plain_success_message_is_unchanged() {
        let msg = format_success("Compiled 'a.php' -> 'a'", Duration::from_millis(420), false);
        assert_eq!(msg, "Compiled 'a.php' -> 'a'");
    }

    /// Verifies decorated success text keeps the original message intact and adds
    /// a checkmark plus elapsed seconds.
    #[test]
    fn decorated_success_message_keeps_original_text() {
        let msg = format_success("Compiled 'a.php' -> 'a'", Duration::from_millis(420), true);
        assert!(msg.contains("Compiled 'a.php' -> 'a'"));
        assert!(msg.contains("0.42s"));
    }

    /// Verifies the event line names the verb and detail text.
    #[test]
    fn event_line_contains_verb_and_detail() {
        let line = format_event("Linking", "elephc_pdo (auto-detected)");
        assert!(line.contains("Linking"));
        assert!(line.contains("elephc_pdo (auto-detected)"));
    }

    /// Verifies sub-second phases remain readable instead of rounding to zero seconds.
    #[test]
    fn short_phase_duration_uses_milliseconds() {
        assert_eq!(
            format_phase_duration(Duration::from_micros(1_250)),
            "1.25 ms"
        );
    }

    /// Verifies longer phases use the same compact seconds scale as final success.
    #[test]
    fn long_phase_duration_uses_seconds() {
        assert_eq!(format_phase_duration(Duration::from_millis(1_250)), "1.25 s");
    }

    /// Verifies completed phase lines use the friendly label and elapsed duration.
    #[test]
    fn completed_phase_line_contains_name_and_duration() {
        let line = format_completed_phase("typecheck", Duration::from_millis(42));
        assert!(line.contains('\u{2713}'));
        assert!(line.contains("Checking types"));
        assert!(line.contains("42.00 ms"));
    }

    /// Verifies unknown future phases remain visible instead of rendering blank text.
    #[test]
    fn unknown_phase_label_falls_back_to_internal_name() {
        assert_eq!(phase_label("new-phase"), "new-phase");
    }
}
