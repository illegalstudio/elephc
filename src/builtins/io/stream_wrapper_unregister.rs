//! Purpose:
//! Home of the PHP `stream_wrapper_unregister` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the protocol argument and returns `Bool`.


builtin! {
    name: "stream_wrapper_unregister",
    area: Io,
    params: [protocol: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamWrapperUnregister,
    ),
    summary: "Unregisters a previously registered URL wrapper.",
    php_manual: "function.stream-wrapper-unregister",
}
