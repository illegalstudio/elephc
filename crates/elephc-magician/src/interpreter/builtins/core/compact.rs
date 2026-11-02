//! Purpose:
//! Implements PHP's `compact()` for the eval interpreter.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call`, ahead of the undefined-function refusal.
//!
//! Key details:
//! - Like `get_defined_vars()`, this is a scope intrinsic: the answer depends on the RUNNING
//!   scope's variables, which only the interpreter's call site can see.
//! - Each argument is a variable name or an array of names, nested to any depth. A name with no
//!   visible variable is skipped (php only warns). `$this` is carried by the scope's receiver,
//!   not an entry, so it is skipped here as in `get_defined_vars()`.
//! - Laravel's schema `Blueprint::addColumn()` builds every column with
//!   `compact('type', 'name')`; without it a column's `type` is empty and the grammar looks up
//!   a method called `type`.

use super::super::super::*;

/// Names the interpreter answers itself as scope intrinsics rather than looking them up.
pub(in crate::interpreter) fn eval_is_compact_intrinsic(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case("compact")
}

/// Evaluates `compact(...$names)`.
pub(in crate::interpreter) fn eval_compact_intrinsic(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(args.len())?;
    for arg in args {
        let value = eval_expr(arg.value(), context, scope, values)?;
        result = eval_compact_collect(value, result, scope, values)?;
        values.release(value)?;
    }
    Ok(result)
}

/// Adds the variables one `compact()` argument names to `result`.
fn eval_compact_collect(
    names: RuntimeCellHandle,
    mut result: RuntimeCellHandle,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(names)? {
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(names)?;
            let Ok(name) = String::from_utf8(bytes.clone()) else {
                return Ok(result);
            };
            if name == "this" {
                return Ok(result);
            }
            if let Some(cell) = scope.visible_cell(&name) {
                let key = values.string_bytes_value(&bytes)?;
                let value = values.retain(cell)?;
                result = values.array_set(result, key, value)?;
            }
            Ok(result)
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let len = values.array_len(names)?;
            for position in 0..len {
                let key = values.array_iter_key(names, position)?;
                let item = values.array_get(names, key)?;
                values.release(key)?;
                result = eval_compact_collect(item, result, scope, values)?;
            }
            Ok(result)
        }
        _ => Ok(result),
    }
}
