//! Purpose:
//! Home of the PHP `hash_hmac` builtin: single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Arity (3–4 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "hash_hmac",
    area: String,
    params: [algo: Str, data: Str, key: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HashHmac,
    ),
    summary: "Generates a keyed hash value using the HMAC method.",
    php_manual: "https://www.php.net/manual/en/function.hash-hmac.php",
}
