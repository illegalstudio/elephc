//! Purpose:
//! Home of the PHP `json_last_error` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `json_last_error` takes no arguments and always
//!   returns `Int`. The registry common path enforces arity before falling back
//!   to `returns`.


builtin! {
    name: "json_last_error",
    area: System,
    params: [],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::JsonLastError,
    ),
    summary: "Returns the last error (if any) occurred during the last JSON encoding/decoding.",
}
