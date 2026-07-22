//! Purpose:
//! Home of the PHP `str_repeat` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `str_repeat` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    name: "str_repeat",
    area: String,
    params: [string: Str, times: Int],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrRepeat,
    ),
    summary: "Repeats a string a given number of times.",
    php_manual: "https://www.php.net/manual/en/function.str-repeat.php",
}
