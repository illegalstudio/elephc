//! Purpose:
//! Home of the PHP `range` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` infers both arguments and always returns `Array(Int)`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_range` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "range",
    area: Array,
    params: [start: Mixed, end: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Create an array containing a range of elements.",
    php_manual: "https://www.php.net/manual/en/function.range.php",
}

/// Infers both arguments and returns `Array(Int)`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Both arguments are inferred for side-effect tracking; the return type is always
/// an indexed integer array matching the runtime emitter's output shape.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    Ok(PhpType::Array(Box::new(PhpType::Int)))
}

/// Lowers a `range` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_range(ctx, inst)
}
