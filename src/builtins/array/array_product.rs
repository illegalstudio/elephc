//! Purpose:
//! Home of the PHP `array_product` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` computes the actual return type (Int or Float) based on the element type
//!   of the argument array. The declared `returns: Int` is only used as the FCC type.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_product` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_product",
    area: Array,
    params: [array: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Calculate the product of values in an array.",
    php_manual: "https://www.php.net/manual/en/function.array-product.php",
}

/// Computes the return type (Int or Float) based on the array element type.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// A float-element array yields Float; integer or mixed-element arrays yield Int.
/// Non-array arguments are rejected.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(ref elem_ty) if **elem_ty == PhpType::Float => Ok(PhpType::Float),
        PhpType::Array(_) => Ok(PhpType::Int),
        PhpType::AssocArray { ref value, .. } if **value == PhpType::Float => Ok(PhpType::Float),
        PhpType::AssocArray { .. } => Ok(PhpType::Int),
        _ => Err(CompileError::new(
            cx.span,
            "array_product() argument must be array",
        )),
    }
}

/// Lowers an `array_product` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_product(ctx, inst)
}
