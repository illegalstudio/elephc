//! Purpose:
//! Home of the PHP `pow` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `pow` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.


builtin! {
    name: "pow",
    area: Math,
    params: [num: Mixed, exponent: Mixed],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Pow,
    ),
    summary: "Exponential expression.",
    php_manual: "https://www.php.net/manual/en/function.pow.php",
}
