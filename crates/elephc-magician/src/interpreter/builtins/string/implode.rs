//! Purpose:
//! Declarative eval registry entry for `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - The current eval implementation supports the two-argument runtime form.
//! - `join` is php's documented ALIAS of `implode` and shares this implementation. It had an AOT
//!   contract and no eval binding, which is the `memory_get_usage()` failure shape one step
//!   removed: `join()` worked in compiled code and was an uncatchable
//!   `call to undefined function join()` inside an eval fragment. It is called from
//!   `twig/src/Extension/CoreExtension.php` and `var-dumper/Dumper/HtmlDumper.php`, both of
//!   which a served request loads.

eval_builtin! {
    contract: "implode",
    area: String,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}

eval_builtin! {
    contract: "join",
    area: String,
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
    let (separator, array) = match args {
        [separator, array] => (
            eval_expr(separator, context, scope, values)?,
            eval_expr(array, context, scope, values)?,
        ),
        [array] => (values.string("")?, eval_expr(array, context, scope, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_implode_result(separator, array, context, values)
}

/// Joins array values in eval iteration order using PHP string conversion.
///
/// An object element joins through its `__toString()`, as in any string context: the plain byte
/// conversion has no object arm and joined Doctrine's `Expr\Select` parts as empty strings.
pub(in crate::interpreter) fn eval_implode_result(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
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
        let element = values.array_get(array, key)?;
        // The key and the element read are this iteration's owners; keeping them leaked two
        // cells per element. `__toString()` hands back a fresh string cell of its own.
        values.release(key)?;
        let text = eval_string_context_value(element, context, values)?;
        let bytes = values.string_bytes(text)?;
        if text.as_ptr() != element.as_ptr() {
            values.release(text)?;
        }
        values.release(element)?;
        output.extend_from_slice(&bytes);
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
        [separator, array] => eval_implode_result(*separator, *array, context, values),
        [array] => {
            let separator = values.string("")?;
            eval_implode_result(separator, *array, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
