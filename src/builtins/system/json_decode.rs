//! Purpose:
//! Home of the PHP `json_decode` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates the json argument type, the optional associative
//!   argument type, and that depth/flags are integers. Type errors are reported at
//!   the offending argument's span (not the call span).

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::system::json_support;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "json_decode",
    area: System,
    params: [
        json: Str,
        associative: Bool = DefaultSpec::Null,
        depth: Int = DefaultSpec::Int(512),
        flags: Int = DefaultSpec::Int(0),
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Decodes a JSON string.",
}

/// Validates the json argument is string-compatible, the associative argument is
/// bool-compatible or null, and depth/flags are integers.
///
/// Reports type errors at the span of the offending argument, not the call span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let json_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !json_support::is_json_string_arg_type(&json_ty) {
        return Err(CompileError::new(
            cx.args[0].span,
            "json_decode() json argument must be string-compatible",
        ));
    }
    if let Some(assoc) = cx.args.get(1) {
        let assoc_ty = cx.checker.infer_type(assoc, cx.env)?;
        if !json_support::is_json_associative_arg_type(&assoc_ty) {
            return Err(CompileError::new(
                assoc.span,
                "json_decode() associative argument must be bool-compatible or null",
            ));
        }
    }
    for extra in cx.args.iter().skip(2) {
        let ty = cx.checker.infer_type(extra, cx.env)?;
        if ty != PhpType::Int {
            return Err(CompileError::new(
                extra.span,
                "json_decode() depth and flags must be integers",
            ));
        }
    }
    Ok(PhpType::Mixed)
}

/// Lowers a `json_decode` call by dispatching to the shared JSON emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::json::lower_json_decode(ctx, inst)
}
