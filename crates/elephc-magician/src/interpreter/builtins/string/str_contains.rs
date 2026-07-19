//! Purpose:
//! Declarative eval registry entry for `str_contains`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-search predicate hook.

eval_builtin! {
    name: "str_contains",
    area: String,
    params: [haystack, needle],
    direct: StringSearch,
    values: StringSearch,
}

use super::super::super::*;

/// Evaluates PHP `str_contains(...)` over haystack and needle expressions.
pub(in crate::interpreter) fn eval_builtin_str_contains(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::str_contains::eval_builtin_string_search_named("str_contains", args, context, scope, values)
}

/// Applies PHP `str_contains(...)` to evaluated haystack and needle values.
pub(in crate::interpreter) fn eval_str_contains_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::str_contains::eval_string_search_named_result("str_contains", haystack, needle, values)
}

/// Evaluates one named PHP byte-string search predicate.
pub(in crate::interpreter) fn eval_builtin_string_search_named(
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
    eval_string_search_named_result(name, haystack, needle, values)
}

/// Checks one converted haystack for one converted needle using PHP byte-string semantics.
pub(in crate::interpreter) fn eval_string_search_named_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let matched = match name {
        "str_contains" => {
            needle.is_empty()
                || haystack
                    .windows(needle.len())
                    .any(|window| window == needle)
        }
        "str_starts_with" => haystack.starts_with(&needle),
        "str_ends_with" => haystack.ends_with(&needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(matched)
}
