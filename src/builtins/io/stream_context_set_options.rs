//! Purpose:
//! Home of the PHP `stream_context_set_options` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The two-argument options form shares the `stream_context_set_option` runtime store.
//! - The PHP 8.6 prelude wrapper additionally refreshes the per-context dispatch map.

builtin! {
    contract: "stream_context_set_options",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamContextSetOptions,
    ),
}
