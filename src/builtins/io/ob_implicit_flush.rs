//! Purpose:
//! Home of the PHP `ob_implicit_flush` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The stored flag is semantically inert in elephc: terminal writes are
//! -   unbuffered syscalls, so implicit flushing is always effectively on.
//! - Returns `true` like PHP 8.
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_implicit_flush`.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ob_implicit_flush",
    area: Io,
    params: [enable: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    lower: lower,
    summary: "Turns implicit flush on/off.",
    php_manual: "function.ob-implicit-flush",
}

/// Lowers an `ob_implicit_flush` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_implicit_flush(ctx, inst)
}
