//! Purpose:
//! Home of the PHP `settype` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The first parameter `var` is passed by reference (mutating builtin); `ref_params[0]`
//!   is set by the `ref` marker in the `builtin!` declaration.
//! - `lazy_check: true` so the check hook controls argument inference order: it infers
//!   `var` then `type` in source order (once each), matching legacy exactly-once inference.
//! - `check` validates that the second argument is a string and returns `Bool`.
//! - `lower` is a thin wrapper over the EIR types-module settype emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "settype",
    area: Types,
    params: [ref var: Mixed, type: Str],
    returns: Bool,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Sets the type of a variable.",
    php_manual: "function.settype",
}

/// Validates the `settype` arguments: infers both in source order and rejects a non-string type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if ty != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "settype() second argument must be a string",
        ));
    }
    Ok(PhpType::Bool)
}

/// Lowers a `settype` call by dispatching to the EIR types-module emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::types::lower_settype(ctx, inst)
}
