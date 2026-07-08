//! Purpose:
//! Declarative eval registry entry for `strpos`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-position hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "strpos",
    area: String,
    params: [haystack, needle, offset = EvalBuiltinDefaultValue::Int(0)],
    direct: StringPosition,
    values: StringPosition,
}

use super::super::super::*;

/// Evaluates PHP `strpos(...)` over haystack and needle expressions.
pub(in crate::interpreter) fn eval_builtin_strpos(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strpos::eval_builtin_string_position_named("strpos", args, context, scope, values)
}

/// Applies PHP `strpos(...)` to evaluated haystack and needle values.
pub(in crate::interpreter) fn eval_strpos_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strpos::eval_string_position_named_result("strpos", haystack, needle, values)
}

/// Evaluates one named PHP byte-string position builtin.
pub(in crate::interpreter) fn eval_builtin_string_position_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_position_named_result(name, haystack, needle, values)
}

/// Returns the first or last byte offset of a converted needle, or PHP `false`.
pub(in crate::interpreter) fn eval_string_position_named_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = match name {
        "strpos" if needle.is_empty() => Some(0),
        "strpos" => haystack
            .windows(needle.len())
            .position(|window| window == needle),
        "strrpos" if needle.is_empty() => Some(haystack.len()),
        "strrpos" => haystack
            .windows(needle.len())
            .rposition(|window| window == needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    match position {
        Some(position) => {
            let position = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(position)
        }
        None => values.bool_value(false),
    }
}
