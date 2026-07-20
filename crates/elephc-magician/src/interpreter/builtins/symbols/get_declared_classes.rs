//! Purpose:
//! Eval registry entry and implementation for `get_declared_classes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - The shared `get_declared_*` array builder lives here because classes are
//!   the primary declaration table and the interface/trait variants reuse it.

eval_builtin! {
    name: "get_declared_classes",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_declared_classes()` calls.
pub(in crate::interpreter) fn eval_get_declared_classes_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_declared_symbols("get_declared_classes", args, context, values)
}

/// Evaluates materialized `get_declared_classes()` arguments.
pub(in crate::interpreter) fn eval_get_declared_classes_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        eval_get_declared_symbols_result("get_declared_classes", context, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Evaluates `get_declared_classes/interfaces/traits()` for eval-visible declarations.
pub(in crate::interpreter) fn eval_builtin_get_declared_symbols(
    name: &str,
    args: &[EvalExpr],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_get_declared_symbols_result(name, context, values)
}

/// Builds an indexed array for eval-visible declared class-like names.
pub(in crate::interpreter) fn eval_get_declared_symbols_result(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "get_declared_classes" => {
            eval_dynamic_string_array_result(context.declared_class_names(), values)
        }
        "get_declared_interfaces" => {
            eval_dynamic_string_array_result(context.declared_interface_names(), values)
        }
        "get_declared_traits" => {
            eval_dynamic_string_array_result(context.declared_trait_names(), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds one indexed PHP array from runtime-owned strings.
fn eval_dynamic_string_array_result(
    items: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}
