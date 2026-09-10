//! Purpose:
//! Binds mb_ereg_replace_callback to the shared mbregex replacement operation.
//!
//! Called from:
//! - The AOT builtin registry and typed EIR lowering.
//!
//! Key details:
//! - The neutral contract owns the signature; protected shared invocation owns coercion and settings.

builtin! {
    contract: "mb_ereg_replace_callback",
    check: crate::builtins::mbstring_callback::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEregReplaceCallback),
}
