//! Purpose:
//! Home of the PHP `atan2` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `atan2` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.


builtin! {
    name: "atan2",
    area: Math,
    params: [y: Float, x: Float],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Atan2,
    ),
    summary: "Returns the arc tangent of two variables.",
    php_manual: "https://www.php.net/manual/en/function.atan2.php",
}
