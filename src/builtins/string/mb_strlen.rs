//! Purpose:
//! Home of the PHP `mb_strlen` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Omitted or null encoding arguments use the shared request's internal encoding.
//! - The optional mbstring bridge owns codec lookup, counting, and diagnostics.

builtin! {
    contract: "mb_strlen",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbStrlen,
    ),
}
