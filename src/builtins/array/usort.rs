//! Purpose:
//! Home of the PHP `usort` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` derives the comparator element type from the array value type and validates both
//!   callback parameters contextually, including object and opaque element types. Returns `Void`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_usort` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "usort",
    area: Array,
    params: [ref array: Mixed, callback: Mixed],
    returns: Void,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Sorts an array by values using a user-defined comparison function.",
    php_manual: "https://www.php.net/manual/en/function.usort.php",
}

/// Validates the array and comparator callback arguments for a `usort` call.
///
/// Infers the array value element type and validates both comparator arguments with that type.
/// Closure bodies receive contextual hints for unannotated
/// parameters, while `Mixed`/`Never` elements leave explicit declarations authoritative.
/// Arity (exactly 2) is pre-validated by the registry. Returns `Ok(PhpType::Void)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let cmp_ty = crate::types::checker::builtins::array_element_type(&arr_ty);
    let label = format!("{}() callback", cx.name);
    let callback_arg_types = [cmp_ty.clone(), cmp_ty];
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &callback_arg_types,
        cx.span,
        cx.env,
        &label,
    )?;
    Ok(PhpType::Void)
}

/// Lowers a `usort` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_usort(ctx, inst)
}
