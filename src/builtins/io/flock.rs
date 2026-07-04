//! Purpose:
//! Home of the PHP `flock` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the stream resource, checks that `operation` is strictly `Int`
//!   (not just accepts_int), and verifies that `would_block` (when present) is passed
//!   as a variable — both checks match the legacy behaviour exactly.
//! - `would_block` is a by-reference parameter (`ref` marker in `params:`); the hook's
//!   variable check is in addition to, not instead of, the ref-ness.
//! - Arguments are pre-inferred by the registry before the hook runs; `operation` is
//!   re-inferred inside the hook to obtain its type for validation.
//! - `lower` is a thin wrapper over `io::lower_flock` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "flock",
    area: Io,
    params: [stream: Mixed, operation: Int, ref would_block: Mixed = DefaultSpec::Null],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Portable advisory file locking.",
    php_manual: "function.flock",
}

/// Validates the stream resource, enforces strict Int type for operation, and
/// requires that `would_block` (if provided) is passed as a plain variable.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    let op_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;  // re-infer to obtain the type
    if op_ty != PhpType::Int {                                  // STRICT eq (not accepts_int)
        return Err(CompileError::new(
            cx.args[1].span,
            "flock() operation must be int",
        ));
    }
    if let Some(arg2) = cx.args.get(2) {
        if !matches!(arg2.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                arg2.span,
                "flock() parameter $would_block must be passed a variable",
            ));
        }
    }
    Ok(PhpType::Bool)
}

/// Lowers a `flock` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_flock(ctx, inst)
}
