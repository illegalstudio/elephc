//! Purpose:
//! Home of the PHP `strcspn` builtin and its runtime semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Shares the `__rt_strspn` helper with `strspn`; the lowering selects the mode.
//! - The result is always an `int`, so the contract's declared return type is authoritative.

builtin! {
    contract: "strcspn",
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Strcspn),
}
