//! Purpose:
//! Home of the PHP `array_rand` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the argument is an array and returns `Int` (the randomly
//!   selected integer index). The declared `returns: Mixed` is the FCC type.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_rand` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_rand",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Pick one or more random keys out of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-rand.php",
}

/// Validates that the argument is an array and returns `Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// The runtime always returns a single random integer index from the array.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_rand() argument must be array",
        ));
    }
    Ok(PhpType::Int)
}

/// Lowers an `array_rand` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_rand(ctx, inst)
}
