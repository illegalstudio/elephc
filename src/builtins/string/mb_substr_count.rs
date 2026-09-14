//! Purpose:
//! Binds `mb_substr_count` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The compiler builtin registry.
//!
//! Key details:
//! - Encoding, Unicode, state, and errors belong to the optional shared engine.

builtin! {
    contract: "mb_substr_count",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbSubstrCount),
}
