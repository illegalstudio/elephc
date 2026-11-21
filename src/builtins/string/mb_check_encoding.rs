//! Purpose:
//! Binds `mb_check_encoding` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The compiler builtin registry.
//!
//! Key details:
//! - Encoding semantics and argument defaults remain in the shared engine and contract.
//! - A literal UTF-8 check requires the bridge only for extension identity; EIR feature
//!   discovery enables the bridge runtime when the call cannot use the native validator.

builtin! {
    contract: "mb_check_encoding",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbCheckEncoding),
    requirements: crate::builtins::requirements::mb_check_encoding_requirements,
}
