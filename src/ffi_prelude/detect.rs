//! Purpose:
//! Decides whether a program references PHP's `ext/ffi` surface, so the FFI prelude and
//! the native bridge are only pulled into programs that use them.
//!
//! Called from:
//! - `crate::ffi_prelude::inject_if_used` and the autoload probe.
//!
//! Key details:
//! - Both type positions (`FFI::`, `new FFI`, hints, `instanceof`, `catch`, `extends`) and
//!   string literals (`class_exists('FFI')`) count, because the class can be reached
//!   dynamically.
//! - Only computed class construction (`new $class`) opts in without naming FFI, because the
//!   variable may hold the string `FFI`. Symbol-table enumeration, computed class/function
//!   probes, opaque eval, and unreadable runtime includes do not; like every other optional
//!   prelude, FFI is present only when the program names it or opts in with `--with-ffi`.

use crate::parser::ast::Stmt;

/// Returns whether the program references the FFI class surface.
pub(super) fn program_uses_ffi(program: &[Stmt]) -> bool {
    // Earlier preludes are injected before this probe runs and contain dynamic calls of
    // their own (`$handler(...)`, `new $class`), which would otherwise mark every
    // XML/DOM/PDO program as introspecting and pull the whole FFI surface in. Only the
    // user's own statements can opt into FFI.
    let user_program = program
        .iter()
        .filter(|stmt| stmt.source_mode != crate::source::SourceMode::Internal)
        .cloned()
        .collect::<Vec<_>>();
    let usage = crate::prelude_prune::usage::collect(&user_program);
    if usage.ffi_introspects {
        return true;
    }
    usage
        .classes
        .iter()
        .chain(usage.literals.iter())
        .any(|name| {
            name == "ffi"
                || name
                    .strip_prefix("ffi\\")
                    .is_some_and(|rest| matches!(rest, "cdata" | "ctype" | "exception" | "parserexception"))
        })
}
