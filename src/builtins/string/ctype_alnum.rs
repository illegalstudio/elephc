//! Purpose:
//! Home of the PHP `ctype_alnum` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ctype_alnum` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    name: "ctype_alnum",
    area: String,
    params: [text: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CtypeAlnum,
    ),
    summary: "Checks if all characters in the string are alphanumeric.",
    php_manual: "https://www.php.net/manual/en/function.ctype-alnum.php",
}
