//! Purpose:
//! Declarative eval registry entry for `strtolower`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-case hook.
//! - Unchanged string inputs preserve native identity; changed results preserve arbitrary PHP bytes.

eval_builtin! {
    contract: "strtolower",
    area: String,
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
    if bytes.is_empty() && matches!(name, "ucfirst" | "lcfirst") { return values.string_literal(""); }
    let mut changed = false;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                    changed = true;
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                    changed = true;
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
                changed = true;
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
                changed = true;
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    if !changed && values.type_tag(value)? == 1 { values.copy_value(value) }
    else { values.string_bytes_value(&bytes) }
}
