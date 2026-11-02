//! Purpose:
//! Home of the PHP `strcspn` builtin and its backend-neutral runtime target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through the builtin registry.
//!
//! Key details:
//! - Implements php-src byte-span behavior over a saturating offset and nullable length window.

builtin! {
    contract: "strcspn",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strcspn,
    ),
}
