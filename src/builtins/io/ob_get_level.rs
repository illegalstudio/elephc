//! Purpose:
//! Home of the PHP `ob_get_level` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Int`, the nesting depth, 0 = no buffering)
//!   is fully determined by the declaration.


builtin! {
    name: "ob_get_level",
    area: Io,
    params: [],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObGetLevel,
    ),
    summary: "Returns the nesting level of the output buffering mechanism.",
    php_manual: "function.ob-get-level",
}
