//! Purpose:
//! Declarative eval registry entry for `str_split`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-split hook.

eval_builtin! {
    contract: "str_split",
    area: String,
    direct: StrSplit,
    values: StrSplit,
}

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Evaluates PHP `str_split(...)` over one string and optional chunk length.
pub(in crate::interpreter) fn eval_builtin_str_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_str_split_result(value, None, values)
        }
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_split_result(value, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits one byte string into indexed string chunks using PHP `str_split()` rules.
pub(in crate::interpreter) fn eval_str_split_result(
    value: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let length = match length {
        Some(length) => eval_int_value(length, values)?,
        None => 1,
    };
    if length <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = EvalArrayBuilder::indexed(values, 0)?;
    for (index, chunk) in bytes.chunks(length).enumerate() {
        result.index(index, |values| values.string_bytes_value(chunk))?;
    }
    Ok(result.finish())
}
