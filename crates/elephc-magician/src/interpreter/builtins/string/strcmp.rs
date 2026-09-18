//! Purpose:
//! Declarative eval registry entry for `strcmp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-compare hook.

eval_builtin! {
    contract: "strcmp",
    area: String,
    direct: StringCompare,
    values: StringCompare,
}

use super::super::super::*;

/// Evaluates PHP `strcmp(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_strcmp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strcmp::eval_builtin_string_compare_named("strcmp", args, context, scope, values)
}

/// Applies PHP `strcmp(...)` to two evaluated string values.
pub(in crate::interpreter) fn eval_strcmp_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strcmp::eval_string_compare_named_result("strcmp", left, right, values)
}

/// Evaluates one named PHP string comparison builtin.
pub(in crate::interpreter) fn eval_builtin_string_compare_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_string_compare_named_result(name, left, right, values)
}

/// Compares two converted strings and returns PHP's raw first-byte difference.
pub(in crate::interpreter) fn eval_string_compare_named_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut left = values.string_bytes(left)?;
    let mut right = values.string_bytes(right)?;
    match name {
        "strcmp" => {}
        "strcasecmp" => {
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let result = compare_byte_slices(&left, &right);
    values.int(result)
}

/// Compares two byte slices using php-src's unsigned-byte difference convention.
pub(in crate::interpreter) fn compare_byte_slices(left: &[u8], right: &[u8]) -> i64 {
    for (left_byte, right_byte) in left.iter().zip(right) {
        let difference = i64::from(*left_byte) - i64::from(*right_byte);
        if difference != 0 {
            return difference;
        }
    }
    match left.len().cmp(&right.len()) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
