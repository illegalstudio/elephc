//! Purpose:
//! Home of the PHP `substr_compare` builtin and its runtime semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - An `$offset` past the end of the haystack or a negative `$length` raises php-src's
//!   catchable `ValueError`, so the runtime function is `MAY_THROW`.

builtin! {
    contract: "substr_compare",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SubstrCompare,
    ),
}
