//! Purpose:
//! Home of the PHP `array_reverse` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: reversing preserves the array shape, so the
//!   return type is the (array-or-assoc) input type unchanged. A check hook is
//!   required both to reject non-array arguments and to echo the input type back.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_reverse` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_reverse",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns an array with the elements in reverse order.",
    php_manual: "https://www.php.net/manual/en/function.array-reverse.php",
}

/// Returns the (shape-preserving) array type for an `array_reverse` call.
///
/// Reversing keeps the array shape, so the input array/assoc type is returned
/// unchanged. Non-array arguments are rejected. The argument is re-inferred here;
/// the registry already inferred it once for side effects, and arity is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_reverse() argument must be array",
        ));
    }
    Ok(ty)
}

/// Lowers an `array_reverse` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_reverse(ctx, inst)
}
