//! Purpose:
//! Home of the PHP `exp` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `exp` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    name: "exp",
    area: Math,
    params: [num: Float],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Exp,
    ),
    summary: "Returns e raised to the power of a number.",
    php_manual: "https://www.php.net/manual/en/function.exp.php",
}
