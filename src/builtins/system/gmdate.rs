//! Purpose:
//! Home of the PHP `gmdate` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `gmdate` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The `timestamp` parameter
//!   is optional and defaults to `null` (current time).

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "gmdate",
    area: System,
    params: [format: Str, timestamp: Int = DefaultSpec::Null],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gmdate,
    ),
    summary: "Formats a GMT/UTC date and time.",
}
