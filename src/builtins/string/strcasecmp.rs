//! Purpose:
//! Home of the PHP `strcasecmp` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `strcasecmp` is a pure-data builtin whose return
//!   type (`Int`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    name: "strcasecmp",
    area: String,
    params: [string1: Str, string2: Str],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strcasecmp,
    ),
    summary: "Binary safe case-insensitive string comparison. Returns negative, zero, or positive.",
    php_manual: "https://www.php.net/manual/en/function.strcasecmp.php",
}
