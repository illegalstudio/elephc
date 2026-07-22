//! Purpose:
//! Home of the PHP `hash_init` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `hash_init` accepts only 1 argument (the algorithm name). The `flags`/`key`
//!   parameters from the PHP golden signature are not supported: HASH_HMAC streaming
//!   mode requires passing a secret key and is blocked by `arity_error` and `max_args`.
//! - `min_args: 1, max_args: 1` enforces exactly 1 arg in `check_arity`. The custom
//!   `arity_error` message explains the HMAC streaming restriction to the caller.
//! - Arity validation runs before the `check` hook fires.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "hash_init",
    area: String,
    params: [algo: Str, flags: Int = DefaultSpec::Int(0), key: Str = DefaultSpec::Str("")],
    min_args: 1,
    max_args: 1,
    arity_error: "hash_init() flags/HASH_HMAC streaming mode is not supported; use hash_hmac() for HMAC",
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HashInit,
    ),
    summary: "Initialize an incremental hashing context.",
    php_manual: "https://www.php.net/manual/en/function.hash-init.php",
}
