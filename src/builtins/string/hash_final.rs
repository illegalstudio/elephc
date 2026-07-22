//! Purpose:
//! Home of the PHP `hash_final` builtin: single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Arity (1–2 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "hash_final",
    area: String,
    params: [context: Mixed, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HashFinal,
    ),
    summary: "Finalizes an incremental hash and returns the digest string.",
    php_manual: "https://www.php.net/manual/en/function.hash-final.php",
}
