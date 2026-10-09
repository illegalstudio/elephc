//! Purpose:
//! Home of the PHP `mt_getrandmax` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Always php's `PHP_MT_RAND_MAX`, 2147483647: the largest value `mt_rand()` and `rand()`
//!   return without a range.

builtin! {
    contract: "mt_getrandmax",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MtGetrandmax,
    ),
}
