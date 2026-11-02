//! Purpose:
//! Eval registry entry and implementation for `preg_grep`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Each element is MATCHED as a string but KEPT as the value it was, under its own key, as
//!   php does; `PREG_GREP_INVERT` keeps the elements that do not match.
//! - A pattern php rejects warns and answers false, like the compiled `preg_grep()`.

use super::super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_grep",
    area: Regex,
    direct: PregGrep,
    values: PregGrep,
}

/// php's `PREG_GREP_INVERT`.
const EVAL_PREG_GREP_INVERT: i64 = 1;

/// Evaluates PHP `preg_grep()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_grep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_preg_grep_values_result(&evaluated, context, values)
}

/// Dispatches by-value `preg_grep()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_grep_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (pattern, array, flags) = match evaluated_args {
        [pattern, array] => (*pattern, *array, None),
        [pattern, array, flags] => (*pattern, *array, Some(*flags)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    if !eval_preg_is_array(array, values)? {
        return eval_throw_type_error(
            "preg_grep(): Argument #2 ($array) must be of type array",
            context,
            values,
        );
    }
    let invert = match flags {
        Some(flags) => eval_int_value(flags, values)? & EVAL_PREG_GREP_INVERT != 0,
        None => false,
    };
    let pattern = values.cast_string(pattern)?;
    let Some(regex) = eval_preg_regex(pattern, "preg_grep", values)? else {
        return values.bool_value(false);
    };
    let entries = eval_preg_array_entries(array, values)?;
    let mut out = values.assoc_new(entries.len())?;
    for (key, element) in entries {
        let subject = eval_preg_string_bytes(element, values)?;
        let matched = regex.captures(&subject).is_some();
        if matched != invert {
            out = values.array_set(out, key, element)?;
        }
    }
    Ok(out)
}
