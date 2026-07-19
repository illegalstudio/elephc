//! Purpose:
//! Home of the PHP `array_values` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy return-type rule: the result is an indexed
//!   `Array` whose element type is the input array's value type (the element type
//!   for an indexed array, the value type for an associative array). A check hook
//!   is required because the return type depends on the inferred argument type.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_values` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_values",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns all the values of an array, re-indexed numerically.",
    php_manual: "https://www.php.net/manual/en/function.array-values.php",
}

/// Returns the re-indexed value-array type for an `array_values` call.
///
/// The result is an indexed `Array` carrying the input array's value type. The
/// argument is re-inferred here to drive the return type; the registry already
/// inferred it once for side effects, and arity is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem_ty) => Ok(PhpType::Array(elem_ty)),
        PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
        _ => Err(CompileError::new(
            cx.span,
            "array_values() argument must be array",
        )),
    }
}

/// Lowers an `array_values` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_values(ctx, inst)
}
