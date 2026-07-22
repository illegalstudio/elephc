//! Purpose:
//! Home of the PHP `str_starts_with` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `str_starts_with` is a pure-data builtin whose
//!   return type (`Bool`) is fully determined by its declaration. The registry
//!   derives the return type from the `returns:` field without calling a check hook.


builtin! {
    name: "str_starts_with",
    area: String,
    params: [haystack: Str, needle: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrStartsWith,
    ),
    summary: "Checks if a string starts with a given substring.",
    php_manual: "https://www.php.net/manual/en/function.str-starts-with.php",
}
