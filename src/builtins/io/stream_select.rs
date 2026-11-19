//! Purpose:
//! Home of the PHP `stream_select` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers all arguments and returns the shared
//!   contract's `int|false` (false when a descriptor exceeds `FD_SETSIZE`).
//! - `read`, `write`, and `except` are by-reference parameters (`ref` marker) for parity
//!   with PHP's mutating select semantics and EIR by-ref lowering.


builtin! {
    contract: "stream_select",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSelect,
    ),
}
