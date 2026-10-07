//! Purpose:
//! Declarative eval registry entry and implementation for `str_replace`, shared with
//! `str_ireplace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`, the direct and by-value hooks, and
//!   `crate::interpreter::expressions::calls::eval_call()` for source-level calls that pass
//!   the by-reference `$count`.
//!
//! Key details:
//! - PHP's array forms: an array `$search` applies its entries in order, each to the result
//!   of the previous one; a `$replace` array is consumed in its own iteration order and runs
//!   out to `""`; an empty search entry is skipped; an array `$subject` returns an array with
//!   the same keys whose elements are string-converted (a nested array becomes `"Array"` with
//!   PHP's warning) and replaced. A string `$search` with an array `$replace` throws php-src's
//!   `TypeError`.
//! - `$count` is the total number of replacements across every entry and element.

eval_builtin! {
    contract: "str_replace",
    area: String,
    direct: StrReplace,
    values: StrReplace,
}

use super::super::super::*;

/// PHP's parameter names, in declaration order.
const PARAMETERS: [&str; 4] = ["search", "replace", "subject", "count"];

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
    eval_str_replace_result(name, search, replace, subject, context, values)
}

/// Evaluates a source-level `str_replace(...)`/`str_ireplace(...)` call, binding named
/// arguments and writing the replacement total back through a `$count` argument.
pub(in crate::interpreter) fn eval_builtin_str_replace_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(&PARAMETERS, &evaluated_args, false)?;
        let search = required_evaluated_ref_arg(&bound, 0)?.value;
        let replace = required_evaluated_ref_arg(&bound, 1)?.value;
        let subject = required_evaluated_ref_arg(&bound, 2)?.value;
        let (result, count) =
            eval_str_replace_with_count(name, search, replace, subject, context, values)?;
        if let Some(count_arg) = optional_evaluated_ref_arg(&bound, 3) {
            let target = count_arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal)?;
            let count = values.int(count)?;
            eval_write_direct_ref_target(
                &target,
                count,
                context,
                values,
                Some(ScopeCellOwnership::Owned),
            )?;
        }
        Ok(result)
    })
}

/// Applies `str_replace()`/`str_ireplace()` to already evaluated operands.
pub(in crate::interpreter) fn eval_str_replace_result(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_str_replace_with_count(name, search, replace, subject, context, values)
        .map(|(result, _)| result)
}

/// Applies `str_replace()`/`str_ireplace()` to evaluated operands and returns the result with
/// the total replacement count.
fn eval_str_replace_with_count(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64), EvalStatus> {
    let search_is_array = eval_is_php_array(search, values)?;
    let replace_is_array = eval_is_php_array(replace, values)?;
    if !search_is_array && replace_is_array {
        return eval_throw_type_error(
            &format!(
                "{name}(): Argument #2 ($replace) must be of type string when argument #1 ($search) is a string"
            ),
            context,
            values,
        );
    }
    let pairs = eval_replacement_pairs(search, search_is_array, replace, replace_is_array, values)?;
    let mut count = 0;
    if !eval_is_php_array(subject, values)? {
        let subject = values.string_bytes(subject)?;
        let output = eval_apply_replacement_pairs(name, subject, &pairs, &mut count)?;
        return Ok((values.string_bytes_value(&output)?, count));
    }
    let len = values.array_len(subject)?;
    // The result keeps the subject's keys, so a packed list stays a packed list.
    let mut result = if values.type_tag(subject)? == EVAL_TAG_ARRAY {
        values.array_new(len)?
    } else {
        values.assoc_new(len)?
    };
    for position in 0..len {
        let key = values.array_iter_key(subject, position)?;
        let element = values.array_get(subject, key)?;
        let element = values.string_bytes(element)?;
        let output = eval_apply_replacement_pairs(name, element, &pairs, &mut count)?;
        let output = values.string_bytes_value(&output)?;
        result = values.array_set(result, key, output)?;
    }
    Ok((result, count))
}

/// Returns whether a value is a PHP array (indexed or associative).
fn eval_is_php_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC))
}

/// Flattens `$search` and `$replace` into ordered `(needle, replacement)` byte pairs: a
/// replacement array is consumed in iteration order and runs out to the empty string.
fn eval_replacement_pairs(
    search: RuntimeCellHandle,
    search_is_array: bool,
    replace: RuntimeCellHandle,
    replace_is_array: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, EvalStatus> {
    let needles = if search_is_array {
        eval_string_list(search, values)?
    } else {
        vec![values.string_bytes(search)?]
    };
    let replacements = if replace_is_array {
        eval_string_list(replace, values)?
    } else {
        vec![values.string_bytes(replace)?]
    };
    Ok(needles
        .into_iter()
        .enumerate()
        .map(|(index, needle)| {
            let replacement = if replace_is_array {
                replacements.get(index).cloned().unwrap_or_default()
            } else {
                replacements[0].clone()
            };
            (needle, replacement)
        })
        .collect())
}

/// String-converts every element of an array in iteration order.
fn eval_string_list(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<Vec<u8>>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut list = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let element = values.array_get(array, key)?;
        list.push(values.string_bytes(element)?);
    }
    Ok(list)
}

/// Applies every pair in order to one subject, adding each replacement to `count`. An empty
/// needle is skipped and an empty intermediate result stops the walk, as in php-src.
fn eval_apply_replacement_pairs(
    name: &str,
    mut subject: Vec<u8>,
    pairs: &[(Vec<u8>, Vec<u8>)],
    count: &mut i64,
) -> Result<Vec<u8>, EvalStatus> {
    for (needle, replacement) in pairs {
        if subject.is_empty() {
            break;
        }
        if needle.is_empty() {
            continue;
        }
        subject = eval_replace_all(name, &subject, needle, replacement, count)?;
    }
    Ok(subject)
}

/// Replaces every non-overlapping occurrence of a non-empty needle in one subject.
fn eval_replace_all(
    name: &str,
    subject: &[u8],
    search: &[u8],
    replace: &[u8],
    count: &mut i64,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::with_capacity(subject.len());
    let mut start = 0;
    while let Some(found) = eval_find_replace_match(name, subject, search, start)? {
        output.extend_from_slice(&subject[start..found]);
        output.extend_from_slice(replace);
        start = found + search.len();
        *count += 1;
    }
    output.extend_from_slice(&subject[start..]);
    Ok(output)
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
