//! Purpose:
//! Home of the PHP `array_column` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: the first argument must be an `Array` of
//!   associative arrays; the result is an indexed `Array` of the associative value
//!   type. Other shapes are rejected. A check hook is required because the return type
//!   depends on the inferred argument type.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//!   Note elephc only supports the 2-argument form (`array`, `column_key`).
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_column` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_column",
    area: Array,
    params: [array: Mixed, column_key: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the values from a single column of an array of arrays.",
    php_manual: "https://www.php.net/manual/en/function.array-column.php",
}

/// Returns the extracted-column array type for an `array_column` call.
///
/// The first argument must be an `Array` of associative arrays; the result is an
/// indexed `Array` of the associative value type. Other shapes are rejected. The
/// argument is re-inferred here to drive the return type; the registry already
/// inferred every argument once for side effects, and arity (exactly 2) is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(inner) => match *inner {
            PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
            _ => Err(CompileError::new(
                cx.span,
                "array_column() requires an array of associative arrays",
            )),
        },
        _ => Err(CompileError::new(
            cx.span,
            "array_column() first argument must be array",
        )),
    }
}

/// Lowers an `array_column` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_column(ctx, inst)
}
