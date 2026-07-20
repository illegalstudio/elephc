//! Purpose:
//! Declarative eval registry entry for `ucwords`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the ucwords hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "ucwords",
    area: String,
    params: [string, separators = EvalBuiltinDefaultValue::Bytes(b" \t\r\n\x0c\x0b")],
    direct: Ucwords,
    values: Ucwords,
}

use super::super::super::*;

/// Evaluates PHP `ucwords(...)` over one string and optional separator expression.
pub(in crate::interpreter) fn eval_builtin_ucwords(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_ucwords_result(value, None, values)
        }
        [value, separators] => {
            let value = eval_expr(value, context, scope, values)?;
            let separators = eval_expr(separators, context, scope, values)?;
            eval_ucwords_result(value, Some(separators), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Uppercases ASCII lowercase bytes at the start of words separated by PHP delimiters.
pub(in crate::interpreter) fn eval_ucwords_result(
    value: RuntimeCellHandle,
    separators: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    let separators = match separators {
        Some(separators) => values.string_bytes(separators)?,
        None => b" \t\r\n\x0c\x0b".to_vec(),
    };
    let mut word_start = true;
    for byte in &mut bytes {
        if separators.contains(byte) {
            word_start = true;
        } else if word_start {
            if byte.is_ascii_lowercase() {
                *byte -= b'a' - b'A';
            }
            word_start = false;
        }
    }
    values.string_bytes_value(&bytes)
}
