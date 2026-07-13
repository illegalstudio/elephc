//! Purpose:
//! Declarative eval registry entry for `strstr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the strstr hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "strstr",
    area: String,
    params: [haystack, needle, before_needle = EvalBuiltinDefaultValue::Bool(false)],
    direct: Strstr,
    values: Strstr,
}

use super::super::super::*;

/// Evaluates PHP `strstr(...)` over haystack, needle, and optional prefix mode.
pub(in crate::interpreter) fn eval_builtin_strstr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [haystack, needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            eval_strstr_result(haystack, needle, false, values)
        }
        [haystack, needle, before_needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            let before_needle = eval_expr(before_needle, context, scope, values)?;
            let before_needle = values.truthy(before_needle)?;
            eval_strstr_result(haystack, needle, before_needle, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the suffix or prefix selected by PHP `strstr()`, or `false` when absent.
pub(in crate::interpreter) fn eval_strstr_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    before_needle: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = if needle.is_empty() {
        Some(0)
    } else {
        eval_find_subslice(&haystack, &needle, 0)
    };
    let Some(position) = position else {
        return values.bool_value(false);
    };
    let result = if before_needle {
        &haystack[..position]
    } else {
        &haystack[position..]
    };
    values.string_bytes_value(result)
}

/// Finds `needle` inside `haystack` starting from one byte offset.
pub(in crate::interpreter) fn eval_find_subslice(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
) -> Option<usize> {
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + start)
}
