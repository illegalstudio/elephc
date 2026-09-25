//! Purpose:
//! Binds mb_send_mail to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The compiler builtin registry and typed runtime-call lowering.
//!
//! Key details:
//! - The bridge resolves active language defaults and performs mail transport.

builtin! {
    contract: "mb_send_mail",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbSendMail),
}
