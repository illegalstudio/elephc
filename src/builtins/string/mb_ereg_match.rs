//! Purpose:
//! Binds mb_ereg_match to the shared mbstring contract, coercion, and Oniguruma engine.
//!
//! Called from:
//! - The compiler builtin registry and typed EIR call lowering.
//!
//! Key details:
//! - Raw patterns, PHP encoding aliases, options, and diagnostics belong to the shared engine.

builtin! {
    contract: "mb_ereg_match",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEregMatch),
}
