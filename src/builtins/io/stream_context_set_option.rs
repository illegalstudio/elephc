//! Purpose:
//! Home of the PHP `stream_context_set_option` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers all arguments and returns `Bool`.
//!   PHP accepts two call shapes — (ctx, options_array) or (ctx, wrapper, option, value) —
//!   both accepted inertly.
//! - `lower` is a thin wrapper over `io::lower_stream_context_set_option` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

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
    lower: lower,
    summary: "Sets an option on the specified context.",
    php_manual: "function.stream-context-set-option",
}

/// Lowers a `stream_context_set_option` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_context_set_option(ctx, inst)
}
