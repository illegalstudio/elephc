//! Purpose:
//! Home of the PHP `stream_context_set_option` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers all arguments and returns `Bool`.
//!   PHP accepts two call shapes — (ctx, options_array) or (ctx, wrapper, option, value) —
//!   both accepted inertly.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "stream_context_set_option",
    area: Io,
    params: [
        context: Mixed,
        wrapper_or_options: Mixed,
        option_name: Str = DefaultSpec::Null,
        value: Mixed = DefaultSpec::Null
    ],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamContextSetOption,
    ),
    summary: "Sets an option on the specified context.",
    php_manual: "function.stream-context-set-option",
}
