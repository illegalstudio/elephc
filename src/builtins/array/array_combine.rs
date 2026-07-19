//! Purpose:
//! Home of the PHP `array_combine` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: the result is an associative array whose key
//!   type is derived from the keys-array element type (via
//!   `array_key_type_from_value_type`) and whose value type is the values-array element
//!   type. Both arguments must be indexed arrays. A check hook is required because the
//!   return type depends on the two inferred argument types.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_combine` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::{array_key_type_from_value_type, PhpType};

builtin! {
    name: "array_combine",
    area: Array,
    params: [keys: Mixed, values: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Creates an array by using one array for keys and another for values.",
    php_manual: "https://www.php.net/manual/en/function.array-combine.php",
}

/// Returns the combined associative-array type for an `array_combine` call.
///
/// The key type is derived from the keys-array element type via
/// `array_key_type_from_value_type`, and the value type is the values-array element
/// type. Both arguments must be indexed arrays. They are re-inferred here to drive the
/// return type; the registry already inferred them once for side effects, and arity
/// (exactly 2) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let keys_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let vals_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let key_elem = match keys_ty {
        PhpType::Array(elem) => *elem,
        _ => {
            return Err(CompileError::new(
                cx.span,
                "array_combine() first argument must be array",
            ));
        }
    };
    let val_elem = match vals_ty {
        PhpType::Array(elem) => *elem,
        _ => {
            return Err(CompileError::new(
                cx.span,
                "array_combine() second argument must be array",
            ));
        }
    };
    Ok(PhpType::AssocArray {
        key: Box::new(array_key_type_from_value_type(key_elem)),
        value: Box::new(val_elem),
    })
}

/// Lowers an `array_combine` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_combine(ctx, inst)
}
