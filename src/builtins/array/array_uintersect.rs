//! Purpose:
//! Home of the PHP `array_uintersect` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array1","array2","callback"])` (exactly 3
//!   required params). The legacy CHECK arm also required exactly 3 arguments; no arity
//!   override is needed.
//! - `check` validates the first argument is an indexed array, derives one contextual
//!   comparator type from each input array, and validates the comparator
//!   callback. Returns the first-argument array type.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_uintersect` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_uintersect",
    area: Array,
    params: [array1: Mixed, array2: Mixed, callback: Mixed],
    returns: Mixed,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Computes the intersection of arrays using a callback comparator.",
    php_manual: "https://www.php.net/manual/en/function.array-uintersect.php",
}

/// Validates the comparator callback for an `array_uintersect` call and returns the first-array type.
///
/// The first argument must be an indexed array. The comparator is validated with one
/// contextual element type per input array. Arity (exactly 3 args) is pre-validated by
/// `check_arity`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_)) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    let second_arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let callback_arg_types = [
        crate::types::checker::builtins::array_element_type(&arr_ty),
        crate::types::checker::builtins::array_element_type(&second_arr_ty),
    ];
    let label = format!("{}() comparator", cx.name);
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[2],
        &callback_arg_types,
        cx.span,
        cx.env,
        &label,
    )?;
    Ok(arr_ty)
}

/// Lowers an `array_uintersect` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_uintersect(ctx, inst)
}
