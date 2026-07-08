//! Purpose:
//! Declarative eval registry entry for `strtolower`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-case hook.

eval_builtin! {
    name: "strtolower",
    area: String,
    params: [string],
    direct: StringCase,
    values: StringCase,
}

use super::super::super::*;

/// Evaluates PHP `strtolower(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_strtolower(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strtolower::eval_builtin_string_case_named("strtolower", args, context, scope, values)
}

/// Applies PHP `strtolower(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_strtolower_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strtolower::eval_string_case_named_result("strtolower", value, values)
}

/// Evaluates one named ASCII case-conversion string builtin.
pub(in crate::interpreter) fn eval_builtin_string_case_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_string_case_named_result(name, value, values)
}

/// Converts one eval value through PHP string conversion and ASCII case mapping.
pub(in crate::interpreter) fn eval_string_case_named_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}
