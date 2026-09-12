//! Purpose:
//! Declares comparator-based array intersection and its shared boxed result contract.
//!
//! Called from:
//! - Checker, EIR, optimizer and callable consumers through the builtin registry.
//!
//! Key details:
//! - Supports the catalogued two-array form, preserving first-array keys and value types.

builtin! {
    contract: "array_uintersect",
    check: super::set_comparator::check,
    lazy_check: true,
    semantics: super::set_comparator::semantics(crate::ir::RuntimeFnId::ArrayUintersect),
}
