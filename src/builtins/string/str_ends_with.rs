//! Purpose:
//! Home of the PHP `str_ends_with` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `str_ends_with` is a pure-data builtin whose
//!   return type (`Bool`) is fully determined by its declaration. The registry
//!   derives the return type from the `returns:` field without calling a check hook.


builtin! {
    name: "str_ends_with",
    area: String,
    params: [haystack: Str, needle: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrEndsWith,
    ),
    summary: "Checks if a string ends with a given substring.",
    php_manual: "https://www.php.net/manual/en/function.str-ends-with.php",
}
