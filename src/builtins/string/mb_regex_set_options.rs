//! Purpose:
//! Binds `mb_regex_set_options` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The compiler builtin registry.
//!
//! Key details:
//! - Regex settings, validation, and request lifetime belong to the shared engine.

builtin! {
    contract: "mb_regex_set_options",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbRegexSetOptions),
}
