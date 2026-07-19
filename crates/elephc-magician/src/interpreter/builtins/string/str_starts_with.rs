//! Purpose:
//! Declarative eval registry entry for `str_starts_with`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-search predicate hook.

eval_builtin! {
    name: "str_starts_with",
    area: String,
    params: [haystack, needle],
    direct: StringSearch,
    values: StringSearch,
}

use super::super::super::*;

/// Evaluates PHP `str_starts_with(...)` over haystack and needle expressions.
pub(in crate::interpreter) fn eval_builtin_str_starts_with(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::str_contains::eval_builtin_string_search_named("str_starts_with", args, context, scope, values)
}

/// Applies PHP `str_starts_with(...)` to evaluated haystack and needle values.
pub(in crate::interpreter) fn eval_str_starts_with_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::str_contains::eval_string_search_named_result("str_starts_with", haystack, needle, values)
}
