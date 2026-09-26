//! Purpose:
//! Binds mb_split to the shared mbregex splitting operation.
//!
//! Called from:
//! - The AOT builtin registry and typed EIR lowering.
//!
//! Key details:
//! - The neutral contract owns the signature; the shared engine owns limits and diagnostics.

builtin! {
    contract: "mb_split",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbSplit),
}
