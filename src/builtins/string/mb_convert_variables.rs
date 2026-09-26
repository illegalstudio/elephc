//! Purpose:
//! Binds mb_convert_variables to the shared live-variable conversion contract.
//!
//! Called from:
//! - Compiler builtin checking and typed EIR runtime-call lowering.
//!
//! Key details:
//! - All variable arguments are by reference, including the variadic tail.
//! - The protected V6 host owns live storage traversal and PHP array COW.

builtin! {
    contract: "mb_convert_variables",
    check: crate::builtins::mbstring::capture_check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbConvertVariables),
}
