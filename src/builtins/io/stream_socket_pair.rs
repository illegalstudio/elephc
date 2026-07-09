//! Purpose:
//! Home of the PHP `stream_socket_pair` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the three Int arguments and returns `Mixed`.
//! - PHP returns `array|false`; the builtin emitter widens the success array's slots through
//!   `__rt_array_to_mixed` so the value flows through Mixed pipelines without per-call
//!   special-casing. `Mixed` for the static type keeps every consumer happy.
//! - `lower` dispatches to `io::lower_stream_socket_pair` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "stream_socket_pair",
    area: Io,
    params: [domain: Int, type: Int, protocol: Int],
    returns: Mixed,
    lower: lower,
    summary: "Creates a pair of connected, indistinguishable socket streams.",
    php_manual: "function.stream-socket-pair",
}

/// Lowers a `stream_socket_pair` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_socket_pair(ctx, inst)
}
