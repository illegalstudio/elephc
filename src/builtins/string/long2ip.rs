//! Purpose:
//! Home of the PHP `long2ip` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `long2ip` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    name: "long2ip",
    area: String,
    params: [ip: Int],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Long2ip,
    ),
    summary: "Converts an IPv4 address from long integer to dotted string notation.",
    php_manual: "https://www.php.net/manual/en/function.long2ip.php",
}
