//! Purpose:
//! Home of the PHP `htmlspecialchars` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `htmlspecialchars` is a pure-data builtin whose
//!   return type (`Str`) is fully determined by its declaration. The registry derives
//!   the return type from the `returns:` field without calling a check hook.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "htmlspecialchars",
    area: String,
    params: [string: Str, flags: Int = DefaultSpec::Int(11), encoding: Str = DefaultSpec::Str("UTF-8")],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Htmlspecialchars,
    ),
    summary: "Converts the HTML special characters in a string into their entities.",
    php_manual: "https://www.php.net/manual/en/function.htmlspecialchars.php",
}
