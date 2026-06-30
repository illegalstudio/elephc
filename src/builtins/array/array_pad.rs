//! Purpose:
//! Home of the PHP `array_pad` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: padding preserves the array shape, so the
//!   return type is the (array-or-assoc) first-argument type unchanged. A check hook is
//!   required both to reject a non-array first argument and to echo its type back.
//! - Arity (exactly 3 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_pad` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_pad",
    area: Array,
    params: [array: Mixed, length: Mixed, value: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Pads an array to the specified length with a value.",
    php_manual: "https://www.php.net/manual/en/function.array-pad.php",
}

/// Returns the (shape-preserving) array type for an `array_pad` call.
///
/// Padding keeps the array shape, so the first-argument array/assoc type is returned
/// unchanged. A non-array first argument is rejected. The first argument is re-inferred
/// here; the registry already inferred every argument once for side effects, and arity
/// (exactly 3) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_pad() first argument must be array",
        ));
    }
    Ok(ty)
}

/// Lowers an `array_pad` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_pad(ctx, inst)
}
