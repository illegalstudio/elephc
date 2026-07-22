//! Purpose:
//! Home of the PHP `pi` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `pi` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. It takes no arguments.


builtin! {
    name: "pi",
    area: Math,
    params: [],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Pi,
    ),
    summary: "Gets value of pi.",
    php_manual: "https://www.php.net/manual/en/function.pi.php",
}
