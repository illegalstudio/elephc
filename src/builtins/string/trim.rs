//! Purpose:
//! Home of the PHP `trim` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `trim` is a pure-data builtin. The registry's arity
//!   check (1 required, 1 optional → 1 or 2 args) exactly matches the legacy check-arm
//!   constraint, so no additional validation is needed.


builtin! {
    name: "trim",
    area: String,
    params: [
        string: Str,
        characters: Str = crate::builtins::spec::DefaultSpec::Str(" \n\r\t\u{000b}\u{000c}\0"),
    ],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Trim,
    ),
    summary: "Strips whitespace (or other characters) from the beginning and end of a string.",
    php_manual: "https://www.php.net/manual/en/function.trim.php",
}
