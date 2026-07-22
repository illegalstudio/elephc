//! Purpose:
//! Home of the internal `__elephc_strtotime_raw` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - This is an internal builtin (`internal: true`) not exposed as a PHP-visible function.
//!   It is a raw strtotime alias returning a plain integer rather than int|false.
//! - The `arity_error` override preserves the user-facing `strtotime` error message.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "__elephc_strtotime_raw",
    area: System,
    params: [datetime: Str, baseTimestamp: Int = DefaultSpec::Null],
    arity_error: "strtotime() takes 1 or 2 arguments",
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcStrtotimeRaw,
    ),
    summary: "Internal raw strtotime alias returning a plain integer.",
    internal: true,
}
