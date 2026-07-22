//! Purpose:
//! Home of the PHP `lcfirst` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `lcfirst` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    name: "lcfirst",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Lcfirst,
    ),
    summary: "Lowercases the first character of a string.",
    php_manual: "https://www.php.net/manual/en/function.lcfirst.php",
}
