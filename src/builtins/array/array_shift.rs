//! Purpose:
//! Home of the PHP `array_shift` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array"]))`: exactly 1 argument,
//!   the `array` param is by-reference. The `ref` marker is mandatory — it is what makes
//!   by-reference mutation lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - `check` reproduces the legacy rule: `Array(elem)` yields the element type,
//!   `AssocArray { value, .. }` yields the value type, any other type is an error.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_shift` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_shift",
    area: Array,
    params: [ref array: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Shifts an element off the beginning of array.",
    php_manual: "https://www.php.net/manual/en/function.array-shift.php",
}

/// Returns the element type for an `array_shift` call.
///
/// The `array` argument is re-inferred to drive the return type. Arity (exactly 1) is
/// pre-validated by the registry. `Array(elem)` yields the element type; `AssocArray`
/// yields the value type; any other type is a compile error.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem) => Ok(*elem),
        PhpType::AssocArray { value, .. } => Ok(*value),
        _ => Err(CompileError::new(cx.span, "array_shift() argument must be array")),
    }
}

/// Lowers an `array_shift` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_shift(ctx, inst)
}
