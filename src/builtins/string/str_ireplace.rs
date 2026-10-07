//! Purpose:
//! Home of the PHP `str_ireplace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Three scalar operands lower to the `__rt_str_ireplace` runtime helper; array operands
//!   and the by-reference `$count` lower to the `crate::str_replace_prelude` helpers. The
//!   shared contract lives in `super::str_replace_support`.

builtin! {
    contract: "str_ireplace",
    check: super::str_replace_support::check,
    lazy_check: true,
    semantics: super::str_replace_support::semantics(
        crate::ir::RuntimeFnId::StrIreplace,
        super::str_replace_support::lower_str_ireplace,
    ),
}
