//! Purpose:
//! Home of the PHP `json_encode` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates that all flag/depth arguments are integers, reporting
//!   each type error at the offending argument's span (not the call span).

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "json_encode",
    area: System,
    params: [
        value: Mixed,
        flags: Int = DefaultSpec::Int(0),
        depth: Int = DefaultSpec::Int(512),
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the JSON representation of a value.",
}

/// Validates that all flag and depth arguments are integers.
///
/// Reports type errors at the span of the offending argument, not the call span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    for extra in &cx.args[1..] {
        let ty = cx.checker.infer_type(extra, cx.env)?;
        if ty != PhpType::Int {
            return Err(CompileError::new(
                extra.span,
                "json_encode() flags and depth must be integers",
            ));
        }
    }
    Ok(PhpType::Str)
}

/// Lowers a `json_encode` call by dispatching to the shared JSON emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::json::lower_json_encode(ctx, inst)
}
