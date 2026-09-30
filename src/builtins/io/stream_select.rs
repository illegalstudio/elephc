//! Purpose:
//! Home of the PHP `stream_select` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers all arguments and returns the contract's
//!   `Mixed`, which is php's `int|false` — the ready-descriptor count, or `false` when the
//!   underlying `poll`/`select` failed.
//! - `read`, `write`, and `except` are by-reference parameters (`ref` marker) for parity
//!   with PHP's mutating select semantics and EIR by-ref lowering.


builtin! {
    contract: "stream_select",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSelect,
    ),
}
