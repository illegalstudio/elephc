//! Purpose:
//! Home of the PHP `md5` builtin: single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Arity (1–2 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "md5",
    area: String,
    params: [string: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Md5,
    ),
    summary: "Calculates the MD5 hash of a string.",
    php_manual: "https://www.php.net/manual/en/function.md5.php",
}
