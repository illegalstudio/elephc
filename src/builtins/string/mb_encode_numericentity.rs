//! Purpose:
//! Binds `mb_encode_numericentity` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The compiler builtin registry.
//!
//! Key details:
//! - Encoding semantics and argument defaults remain in the shared engine and contract.

builtin! {
    contract: "mb_encode_numericentity",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEncodeNumericentity),
}
