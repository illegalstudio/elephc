//! Purpose:
//! Declarative eval registry entry for `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - Both overloads report PHP's catchable argument TypeErrors.

eval_builtin! {
    contract: "implode",
    area: String,
    source_arguments: true,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}

use super::super::super::*;

/// PHP's overload error when the optional array was not supplied or is null.
const IMPLODE_NULL_TYPE_ERROR: &str = "implode(): If argument #1 ($separator) is of type string, \
argument #2 ($array) must be of type array, null given";

/// Evaluates PHP `implode()` over separator and array expressions.
pub(in crate::interpreter) fn eval_builtin_implode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operands = match args {
        [array] => vec![array],
        [separator, array] => vec![separator, array],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    with_eval_operands(&operands, context, scope, values, |args, context, _, values| {
        // Zend specializes a direct one-argument implode call as an array-only join. It
        // reports this overload error without attempting separator coercion.
        if let [array] = args {
            if !matches!(values.type_tag(*array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                return eval_throw_type_error(IMPLODE_NULL_TYPE_ERROR, context, values);
            }
        }
        eval_implode_declared_values_result(args, context, values)
    })
}

/// Joins array values in eval iteration order using PHP string conversion.
pub(in crate::interpreter) fn eval_implode_result(
    separator: Option<RuntimeCellHandle>,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(array)?;
    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let first_argument = separator.is_none()
            && (matches!(tag, EVAL_TAG_RESOURCE | EVAL_TAG_CALLABLE)
                || (tag == EVAL_TAG_OBJECT
                    && !eval_object_has_to_string(array, context, values)?));
        if separator.is_none() && tag == EVAL_TAG_OBJECT && !first_argument {
            let rendered = eval_string_context_value(array.borrowed(), context, values)?;
            with_eval_value_lease(rendered, context, values, |_, _, _| Ok(()))?;
        }
        let message = if first_argument {
            let given = eval_given_type_name(array, context, values)?;
            format!("implode(): Argument #1 ($separator) must be of type array|string, {given} given")
        } else if separator.is_none() || tag == EVAL_TAG_NULL {
            IMPLODE_NULL_TYPE_ERROR.to_string()
        } else {
            let given = eval_given_type_name(array, context, values)?;
            format!("implode(): Argument #2 ($array) must be of type ?array, {given} given")
        };
        return eval_throw_type_error(&message, context, values);
    }
    let separator = match separator {
        Some(separator) => values.string_bytes(separator)?,
        None => Vec::new(),
    };
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

/// Dispatches evaluated `implode()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_implode_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array] => eval_implode_result(None, *array, context, values),
        [separator, array] => eval_implode_result(Some(*separator), *array, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
