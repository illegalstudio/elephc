//! Purpose:
//! Declarative eval registry entry for `trim`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the trim-family hook.

eval_builtin! {
    contract: "trim",
    area: String,
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

pub(in crate::interpreter) const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\0";

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

/// Expands PHP's trim charlist into a 256-byte membership set.
///
/// Mirrors `php_charmask`: an increasing `start..end` range contributes every
/// byte between its endpoints, while an invalid range leaves its bytes literal
/// instead of consuming the four-byte range syntax. The invalid-range warning
/// is not emitted yet.
fn expand_trim_mask(
    mask: &[u8],
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<[bool; 256], EvalStatus> {
    let mut set = [false; 256];
    let mut index = 0;
    while index < mask.len() {
        let byte = mask[index];
        if index + 3 < mask.len() && mask[index + 1] == b'.' && mask[index + 2] == b'.' {
            let end = mask[index + 3];
            if end >= byte {
                for value in byte..=end {
                    set[value as usize] = true;
                }
                index += 4;
            } else {
                set[byte as usize] = true;
                index += 1;
            }
        } else if byte == b'.' && index + 1 < mask.len() && mask[index + 1] == b'.' {
            let message = if index == 0 {
                "Invalid '..'-range, no character to the left of '..'"
            } else if index + 2 >= mask.len() {
                "Invalid '..'-range, no character to the right of '..'"
            } else if mask[index - 1] > mask[index + 2] {
                "Invalid '..'-range, '..'-range needs to be incrementing"
            } else {
                "Invalid '..'-range"
            };
            values.warning(&format!("Warning: {name}(): {message}\n"))?;
            index += 1;
        } else {
            set[byte as usize] = true;
            index += 1;
        }
    }
    Ok(set)
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
    let trim_set = expand_trim_mask(trim_mask, name, values)?;

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_set[bytes[start] as usize] {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_set[bytes[end - 1] as usize] {
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
