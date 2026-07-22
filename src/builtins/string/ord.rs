//! Purpose:
//! Home of the PHP `ord` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ord` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    name: "ord",
    area: String,
    params: [character: Str],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Ord,
    ),
    summary: "Returns the ASCII value of the first character of a string.",
    php_manual: "https://www.php.net/manual/en/function.ord.php",
}
