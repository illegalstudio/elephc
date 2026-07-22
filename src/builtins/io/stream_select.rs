//! Purpose:
//! Home of the PHP `stream_select` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers all arguments and returns `Int`.
//! - `read`, `write`, and `except` are by-reference parameters (`ref` marker) for parity
//!   with PHP's mutating select semantics and EIR by-ref lowering.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "stream_select",
    area: Io,
    params: [
        ref read: Mixed,
        ref write: Mixed,
        ref except: Mixed,
        seconds: Int,
        microseconds: Int = DefaultSpec::Int(0)
    ],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSelect,
    ),
    summary: "Runs the equivalent of the select() system call on the given arrays of streams.",
    php_manual: "function.stream-select",
}
