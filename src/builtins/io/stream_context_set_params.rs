//! Purpose:
//! Home of the PHP `stream_context_set_params` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers both arguments and returns `Bool`.


builtin! {
    name: "stream_context_set_params",
    area: Io,
    params: [context: Mixed, params: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamContextSetParams,
    ),
    summary: "Sets parameters on the specified context.",
    php_manual: "function.stream-context-set-params",
}
