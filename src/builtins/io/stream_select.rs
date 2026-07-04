//! Purpose:
//! Home of the PHP `stream_select` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers all arguments and returns `Int`.
//! - `read`, `write`, and `except` are by-reference parameters (`ref` marker) for parity
//!   with PHP's mutating select semantics and EIR by-ref lowering.
//! - `lower` is a thin wrapper over `io::lower_stream_select` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_select",
    area: Io,
    params: [
        ref read: Mixed,
        ref write: Mixed,
        ref except: Mixed,
        seconds: Int,
        microseconds: Int = DefaultSpec::Int(0)
    ],
    returns: Int,
    lower: lower,
    summary: "Runs the equivalent of the select() system call on the given arrays of streams.",
    php_manual: "function.stream-select",
}

/// Lowers a `stream_select` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_select(ctx, inst)
}
