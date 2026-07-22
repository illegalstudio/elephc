//! Purpose:
//! Home of the PHP `stream_is_local` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the stream argument and returns `Bool`.


builtin! {
    name: "stream_is_local",
    area: Io,
    params: [stream: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamIsLocal,
    ),
    summary: "Checks if a stream is a local stream.",
    php_manual: "function.stream-is-local",
}
