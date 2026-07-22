//! Purpose:
//! Home of the PHP `fmod` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `fmod` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.


builtin! {
    name: "fmod",
    area: Math,
    params: [num1: Float, num2: Float],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fmod,
    ),
    summary: "Returns the floating point remainder of the division of the arguments.",
    php_manual: "https://www.php.net/manual/en/function.fmod.php",
}
