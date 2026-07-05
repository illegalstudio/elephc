//! Purpose:
//! Home of the PHP `sys_get_temp_dir` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `sys_get_temp_dir` is a pure-data builtin whose `Str` return
//!   type is fully determined by its declaration. The registry common path enforces
//!   its 0-argument arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_sys_get_temp_dir` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "sys_get_temp_dir",
    area: Io,
    params: [],
    returns: Str,
    lower: lower,
    summary: "Returns the directory path used for temporary files.",
    php_manual: "function.sys-get-temp-dir",
}

/// Lowers a `sys_get_temp_dir` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_sys_get_temp_dir(ctx, inst)
}
