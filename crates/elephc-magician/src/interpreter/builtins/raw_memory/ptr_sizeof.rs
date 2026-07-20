//! Purpose:
//! Eval registry entry and implementation for `ptr_sizeof`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Computes the checked byte size for scalar pointer targets and boxed classes.

use super::super::super::*;


eval_builtin! {
    name: "ptr_sizeof",
    area: RawMemory,
    params: [r#type],
    direct: PtrSizeof,
    values: PtrSizeof,
}

/// Evaluates PHP `ptr_sizeof()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ptr_sizeof(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [type_name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let type_name = eval_expr(type_name, context, scope, values)?;
    eval_ptr_sizeof_result(type_name, context, values)
}

/// Dispatches by-value `ptr_sizeof()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_sizeof_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [type_name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_sizeof_result(*type_name, context, values)
}

/// Computes the checked byte size for a low-level type name.
fn eval_ptr_sizeof_result(
    type_name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(type_name)?;
    let type_name = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    let size = eval_pointer_target_size(type_name.trim_start_matches('\\'), context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    values.int(i64::try_from(size).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Returns the eval-side byte size for one low-level pointer target name.
fn eval_pointer_target_size(type_name: &str, context: &ElephcEvalContext) -> Option<usize> {
    match type_name.to_ascii_lowercase().as_str() {
        "int" | "integer" => Some(8),
        "float" | "double" | "real" => Some(8),
        "bool" | "boolean" => Some(8),
        "string" => Some(16),
        "ptr" | "pointer" => Some(8),
        _ => context.class(type_name).map(eval_boxed_class_size),
    }
}

/// Returns the boxed object storage size used by AOT class metadata.
fn eval_boxed_class_size(class: &EvalClass) -> usize {
    let instance_properties = class
        .properties()
        .iter()
        .filter(|property| !property.is_static())
        .count();
    8 + instance_properties * 16
}
