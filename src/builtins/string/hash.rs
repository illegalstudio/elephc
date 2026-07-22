//! Purpose:
//! Home of the PHP `hash` builtin: single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Arity (2–3 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "hash",
    area: String,
    params: [algo: Str, data: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Hash,
    ),
    summary: "Generates a hash value using the given algorithm.",
    php_manual: "https://www.php.net/manual/en/function.hash.php",
}
