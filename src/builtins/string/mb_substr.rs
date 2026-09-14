//! Purpose:
//! Binds `mb_substr` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The compiler builtin registry.
//!
//! Key details:
//! - Encoding, Unicode, state, and errors belong to the optional shared engine.

builtin! {
    contract: "mb_substr",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbSubstr),
}
