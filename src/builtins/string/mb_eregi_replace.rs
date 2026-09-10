//! Purpose:
//! Binds mb_eregi_replace to the shared mbregex replacement operation.
//!
//! Called from:
//! - The AOT builtin registry and typed EIR lowering.
//!
//! Key details:
//! - The neutral contract owns the signature; protected shared invocation owns coercion and settings.

builtin! {
    contract: "mb_eregi_replace",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEregiReplace),
}
