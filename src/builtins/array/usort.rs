//! Purpose:
//! Home of the PHP `usort` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` derives the comparator element type from the array value type and validates both
//!   callback parameters contextually, including object and opaque element types. Returns `Void`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "usort",
    area: Array,
    params: [ref array: Mixed, callback: Mixed],
    returns: Void,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Usort),
        crate::builtins::semantics::BuiltinArgumentLowering::UserValueSort,
    ),
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
