//! Purpose:
//! Declarative eval registry entry for `str_replace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-replace hook.

eval_builtin! {
    contract: "str_replace",
    area: String,
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
    let (search, replace, subject, count) = match args {
        [search, replace, subject] => (search, replace, subject, None),
        [search, replace, subject, count] => (search, replace, subject, Some(count)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let search = eval_expr(search, context, scope, values)?;
    let replace = eval_expr(replace, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    // The fourth argument is php's by-reference replacement count. Symfony's `cache:clear`
    // rewrites every warmed file with `str_replace($search, $replace, $content, $count)` and
    // writes only when `$count` is non-zero; the three-argument shape alone refused it.
    let Some(count) = count else {
        return eval_str_replace_result(name, search, replace, subject, values);
    };
    let target = eval_preg_matches_target(count, context, scope, values)?;
    let mut replacements = 0usize;
    let result =
        eval_str_replace_result_counted(name, search, replace, subject, &mut replacements, values)?;
    let count = values.int(replacements as i64)?;
    eval_write_preg_matches_target(&target, count, context, values)?;
    Ok(result)
}

/// Replaces every non-overlapping occurrence of a needle, over php's four argument shapes.
///
/// php: `$search`, `$replace` and `$subject` are each `array|string`, and the return type
/// follows the SUBJECT — "if `$subject` is an array, then the search and replace is performed
/// with every entry of `$subject`, and the return value is an array as well". Measured on
/// 8.5.10, all with `$search = ['a','b']` unless noted:
///
/// ```text
/// str_replace('a', 'X', 'aba')                 => 'XbX'
/// str_replace(['a','b'], 'X', 'aba')           => 'XXX'    scalar $replace reused per needle
/// str_replace(['a','b'], ['X','Y'], 'aba')     => 'XYX'    positional pairing
/// str_replace(['a','b'], ['X'], 'aba')         => 'XX'     missing replacement is ''
/// str_replace(['a','X'], ['X','Z'], 'a')       => 'Z'      passes are SEQUENTIAL
/// str_replace('a', 'X', ['aa','ba'])           => ['XX','bX']
/// str_replace(['a'], ['X'], ['k'=>'aa'])       => ['k'=>'XX']   subject keys preserved
/// ```
///
/// Only the first line of that table used to work here: every argument went straight through
/// `string_bytes`, so the polyfill-shaped call `str_replace($arr, $arr, $arr)` inside an eval
/// fragment printed three "Array to string conversion" warnings and answered the literal
/// string `"Array"`.
///
/// The sequential ordering is load-bearing and is why this walks the needles over a single
/// running buffer rather than scanning the subject once: an earlier pair's OUTPUT is visible to
/// a later pair's search, which is what makes the fifth line `'Z'` and not `'X'`.
pub(in crate::interpreter) fn eval_str_replace_result(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut replacements = 0usize;
    eval_str_replace_result_counted(name, search, replace, subject, &mut replacements, values)
}

/// Same as [`eval_str_replace_result`], also counting every replacement made.
fn eval_str_replace_result_counted(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    replacements: &mut usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.is_array_like(subject)? {
        let len = values.array_len(subject)?;
        let mut result = values.assoc_new(len)?;
        for position in 0..len {
            let key = values.array_iter_key(subject, position)?;
            let element = values.array_get(subject, key)?;
            let replaced = eval_str_replace_scalar_subject(
                name,
                search,
                replace,
                element,
                replacements,
                values,
            )?;
            result = values.array_set(result, key, replaced)?;
        }
        return Ok(result);
    }
    eval_str_replace_scalar_subject(name, search, replace, subject, replacements, values)
}

/// Applies every search/replacement pair, in order, to one string subject.
fn eval_str_replace_scalar_subject(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    replacements: &mut usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut current = values.string_bytes(subject)?;
    if values.is_array_like(search)? {
        let replace_is_array = values.is_array_like(replace)?;
        let replace_len = if replace_is_array {
            values.array_len(replace)?
        } else {
            0
        };
        let search_len = values.array_len(search)?;
        for position in 0..search_len {
            let search_key = values.array_iter_key(search, position)?;
            let needle = values.array_get(search, search_key)?;
            let needle = values.string_bytes(needle)?;
            // php pairs the two arrays POSITIONALLY and ignores their keys, and a needle past
            // the end of `$replace` is replaced with the empty string, not skipped.
            let replacement = if replace_is_array {
                if position < replace_len {
                    let replace_key = values.array_iter_key(replace, position)?;
                    let element = values.array_get(replace, replace_key)?;
                    values.string_bytes(element)?
                } else {
                    Vec::new()
                }
            } else {
                values.string_bytes(replace)?
            };
            current = eval_replace_all_in_bytes(name, &current, &needle, &replacement, replacements)?;
        }
    } else {
        // php refuses the mixed form outright: `str_replace('a', ['X'], 'aba')` is a TypeError,
        // "Argument #2 ($replace) must be of type string when argument #1 ($search) is a
        // string". Nothing sensible can be produced, so this is the eval fatal.
        if values.is_array_like(replace)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let needle = values.string_bytes(search)?;
        let replacement = values.string_bytes(replace)?;
        current = eval_replace_all_in_bytes(name, &current, &needle, &replacement, replacements)?;
    }
    values.string_bytes_value(&current)
}

/// Replaces every non-overlapping occurrence of one byte needle in one byte subject.
fn eval_replace_all_in_bytes(
    name: &str,
    subject: &[u8],
    search: &[u8],
    replace: &[u8],
    replacements: &mut usize,
) -> Result<Vec<u8>, EvalStatus> {
    // php leaves the subject untouched for an empty needle rather than splicing the
    // replacement between every byte: `str_replace('', 'X', 'ab')` is `'ab'`.
    if search.is_empty() {
        return Ok(subject.to_vec());
    }
    let mut output = Vec::with_capacity(subject.len());
    let mut start = 0;
    while let Some(found) = eval_find_replace_match(name, subject, search, start)? {
        output.extend_from_slice(&subject[start..found]);
        output.extend_from_slice(replace);
        *replacements += 1;
        start = found + search.len();
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
