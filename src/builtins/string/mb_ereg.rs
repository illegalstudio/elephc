//! Purpose:
//! Binds mb_ereg to shared PHP coercion, capture construction, and native reference adaptation.
//!
//! Called from:
//! - Compiler builtin checking and typed EIR runtime-call lowering.
//!
//! Key details:
//! - The neutral contract owns the optional by-reference output and boolean result.
//! - The native adapter preserves output identity instead of copying its former value.

builtin! {
    contract: "mb_ereg",
    check: crate::builtins::mbstring::capture_check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEreg),
}
