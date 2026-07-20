//! Purpose:
//! Home of the PHP `ob_list_handlers` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array<Str>` (the macro cannot express array returns inline):
//! -   one "default output handler" entry per active buffer level.
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_list_handlers`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ob_list_handlers",
    area: Io,
    params: [],
    returns: Mixed,
    returns_fresh_storage: true,
    check: check,
    lower: lower,
    summary: "Lists all output handlers in use.",
    php_manual: "function.ob-list-handlers",
}

/// Returns `Array<Str>`: one "default output handler" name per active buffer level.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers an `ob_list_handlers` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_list_handlers(ctx, inst)
}
