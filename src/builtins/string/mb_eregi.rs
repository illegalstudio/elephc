//! Purpose:
//! Binds mb_eregi to the shared case-insensitive capture engine and native reference adapter.
//!
//! Called from:
//! - Compiler builtin checking and typed EIR runtime-call lowering.
//!
//! Key details:
//! - PHP metadata and argument coercion remain shared with the mbstring engine.
//! - Case sensitivity is selected by RuntimeBuiltinId, not by backend name dispatch.

builtin! {
    contract: "mb_eregi",
    check: crate::builtins::mbstring::capture_check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEregi),
}
