//! Purpose:
//! Declarative eval registry entry for `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string split/join hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "implode",
    area: String,
    params: [separator = EvalBuiltinDefaultValue::Null, array],
    required: 1,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}

use super::super::super::*;

/// Evaluates PHP `implode()` over separator and array expressions.
pub(in crate::interpreter) fn eval_builtin_implode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_implode_result(separator, array, values)
}

/// Joins array values in eval iteration order using PHP string conversion.
pub(in crate::interpreter) fn eval_implode_result(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let separator = values.string_bytes(separator)?;
    let len = values.array_len(array)?;
    let mut output = Vec::new();
    for position in 0..len {
        if position > 0 {
            output.extend_from_slice(&separator);
        }
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let value = values.string_bytes(value)?;
        output.extend_from_slice(&value);
    }
    values.string_bytes_value(&output)
}
