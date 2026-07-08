//! Purpose:
//! Declarative eval registry entry for `trim`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the trim-family hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "trim",
    area: String,
    params: [string, characters = EvalBuiltinDefaultValue::Bytes(b" \n\r\t\x0b\x0c\0")],
    direct: TrimLike,
    values: TrimLike,
}

use super::super::super::*;

/// Evaluates PHP `trim(...)` over one eval expression and optional mask.
pub(in crate::interpreter) fn eval_builtin_trim(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trim::eval_builtin_trim_like_named("trim", args, context, scope, values)
}

/// Applies PHP `trim(...)` to one evaluated string and optional mask.
pub(in crate::interpreter) fn eval_trim_result(
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trim::eval_trim_like_named_result("trim", value, mask, values)
}

pub(in crate::interpreter) const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\x0C\0";

/// Evaluates one named PHP trim-family builtin.
pub(in crate::interpreter) fn eval_builtin_trim_like_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_trim_like_named_result(name, value, None, values)
        }
        [value, mask] => {
            let value = eval_expr(value, context, scope, values)?;
            let mask = eval_expr(mask, context, scope, values)?;
            eval_trim_like_named_result(name, value, Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one converted string using PHP's default mask or a caller-provided byte mask.
pub(in crate::interpreter) fn eval_trim_like_named_result(
    name: &str,
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let explicit_mask;
    let trim_mask = if let Some(mask) = mask {
        explicit_mask = values.string_bytes(mask)?;
        explicit_mask.as_slice()
    } else {
        PHP_DEFAULT_TRIM_MASK
    };

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_mask.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_mask.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    if !matches!(name, "trim" | "ltrim" | "rtrim" | "chop") {
        return Err(EvalStatus::UnsupportedConstruct);
    }

    let value =
        String::from_utf8(bytes[start..end].to_vec()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}
