//! Purpose:
//! Home of the PHP `stream_context_set_options` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP 8.3 added this as the two-argument spelling of the array form
//!   `stream_context_set_option($context, $options)`, which the singular name still accepts.
//!   Both reach the same runtime target, so the array shape is applied once and identically.
//! - No check hook: the common registry path infers both arguments and returns `Bool`.

builtin! {
    contract: "stream_context_set_options",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamContextSetOptions,
    ),
}
