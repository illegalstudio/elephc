//! Purpose:
//! Home of the PHP `var_dump` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `var_dump` is a pure-data builtin whose return type
//!   (`Void`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `debug::lower_var_dump` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "var_dump",
    area: Io,
    params: [value: Mixed],
    returns: Void,
    lower: lower,
    summary: "Dumps information about a variable.",
    php_manual: "function.var-dump",
}

/// Lowers a `var_dump` call by dispatching to the shared debug emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::debug::lower_var_dump(ctx, inst)
}
