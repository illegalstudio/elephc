//! Purpose:
//! Parser-side bookkeeping for PHP 8.6 partial function application.
//!
//! Called from:
//! - `crate::syntax::parser::expr` when it writes a `?` or trailing `...` marker.
//! - The compiler's `pfa::desugar_for_version()` pass, which consumes the marker flag.
//!
//! Key details:
//! - The parser only records that a marker was seen; the AST rewrite lives in the
//!   compiler crate, so parsing does not depend on the compiler or its signature tables.
//! - The flag is thread-local because compilations run on worker threads and a marker
//!   from one program must not trigger an unnecessary walk in another.

use std::cell::Cell;

thread_local! {
    /// Set by the parser when it creates a `?` or trailing `...` marker.
    static SAW_PFA_MARKER: Cell<bool> = const { Cell::new(false) };
}

/// Sentinel variable name the parser writes for a `?` argument placeholder.
pub const PLACEHOLDER_VAR: &str = "__elephc_pfa_placeholder";
/// Sentinel variable name the parser writes inside the `Spread` of a `...` placeholder.
pub const VARIADIC_PLACEHOLDER_VAR: &str = "__elephc_pfa_variadic_placeholder";

/// Records that the parser created at least one partial-application marker.
pub fn note_marker() {
    SAW_PFA_MARKER.with(|flag| flag.set(true));
}

/// Returns and clears the marker flag, so a program with no partial application is not walked.
pub fn take_marker() -> bool {
    SAW_PFA_MARKER.with(|flag| flag.replace(false))
}
