//! Purpose:
//! Home of the PHP `hash_copy` builtin: single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Arity (exactly 1 arg) is validated by the registry's `check_arity` before the hook fires.


builtin! {
    name: "hash_copy",
    area: String,
    params: [context: Mixed],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HashCopy,
    ),
    summary: "Copies the state of an incremental hashing context.",
    php_manual: "https://www.php.net/manual/en/function.hash-copy.php",
}
