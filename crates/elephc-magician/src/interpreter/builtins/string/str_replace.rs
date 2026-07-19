//! Purpose:
//! Declarative eval registry entry for `str_replace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-replace hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "str_replace",
    area: String,
    params: [search, replace, subject, count = EvalBuiltinDefaultValue::Null],
    direct: StrReplace,
    values: StrReplace,
}

use super::super::super::*;

/// Evaluates PHP's `str_replace(...)` or `str_ireplace(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_str_replace(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [search, replace, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let search = eval_expr(search, context, scope, values)?;
    let replace = eval_expr(replace, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_str_replace_result(name, search, replace, subject, values)
}

/// Replaces every non-overlapping occurrence of a byte-string needle in a subject.
pub(in crate::interpreter) fn eval_str_replace_result(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let search = values.string_bytes(search)?;
    let replace = values.string_bytes(replace)?;
    let subject = values.string_bytes(subject)?;
    if search.is_empty() {
        return values.string_bytes_value(&subject);
    }

    let mut output = Vec::with_capacity(subject.len());
    let mut start = 0;
    while let Some(found) = eval_find_replace_match(name, &subject, &search, start)? {
        output.extend_from_slice(&subject[start..found]);
        output.extend_from_slice(&replace);
        start = found + search.len();
    }
    output.extend_from_slice(&subject[start..]);
    values.string_bytes_value(&output)
}

/// Finds the next replacement match using case-sensitive or ASCII-insensitive comparison.
pub(in crate::interpreter) fn eval_find_replace_match(
    name: &str,
    subject: &[u8],
    search: &[u8],
    start: usize,
) -> Result<Option<usize>, EvalStatus> {
    match name {
        "str_replace" => Ok(super::strstr::eval_find_subslice(subject, search, start)),
        "str_ireplace" => Ok(subject
            .get(start..)
            .and_then(|tail| {
                tail.windows(search.len())
                    .position(|window| window.eq_ignore_ascii_case(search))
            })
            .map(|position| position + start)),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
