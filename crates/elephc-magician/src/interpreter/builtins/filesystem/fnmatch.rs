//! Purpose:
//! Declarative eval registry entry for `fnmatch`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the fnmatch helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fnmatch",
    area: Filesystem,
    params: [pattern, filename, flags = EvalBuiltinDefaultValue::Int(0)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fnmatch` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fnmatch_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("fnmatch", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fnmatch` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fnmatch_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("fnmatch", evaluated_args, context, values)
}

use super::super::*;

/// Evaluates PHP `fnmatch($pattern, $filename, $flags = 0)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fnmatch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, filename] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let filename = eval_expr(filename, context, scope, values)?;
            eval_fnmatch_result(pattern, filename, None, values)
        }
        [pattern, filename, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let filename = eval_expr(filename, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_fnmatch_result(pattern, filename, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Runs PHP-style shell glob matching for one pattern/name pair.
pub(in crate::interpreter) fn eval_fnmatch_result(
    pattern: RuntimeCellHandle,
    filename: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    let filename = values.string_bytes(filename)?;
    let flags = match flags {
        Some(flags) => eval_int_value(flags, values)?,
        None => 0,
    };
    values.bool_value(eval_fnmatch_bytes(&pattern, &filename, flags))
}

/// Matches byte strings using the eval-supported `fnmatch()` grammar and flags.
pub(in crate::interpreter) fn eval_fnmatch_bytes(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
) -> bool {
    let mut memo = vec![vec![None; filename.len() + 1]; pattern.len() + 1];
    eval_fnmatch_at(pattern, filename, flags, 0, 0, &mut memo)
}

/// Recursively matches a pattern suffix against a filename suffix with memoization.
pub(in crate::interpreter) fn eval_fnmatch_at(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if let Some(result) = memo[pattern_index][filename_index] {
        return result;
    }
    let result = if pattern_index == pattern.len() {
        filename_index == filename.len()
    } else {
        match pattern[pattern_index] {
            b'*' => eval_fnmatch_star(
                pattern,
                filename,
                flags,
                pattern_index,
                filename_index,
                memo,
            ),
            b'?' => {
                eval_fnmatch_single_wildcard(filename, flags, filename_index)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        pattern_index + 1,
                        filename_index + 1,
                        memo,
                    )
            }
            b'[' => eval_fnmatch_class_or_literal(
                pattern,
                filename,
                flags,
                pattern_index,
                filename_index,
                memo,
            ),
            b'\\' if flags & EVAL_FNM_NOESCAPE == 0 => {
                let (literal, next_pattern_index) =
                    eval_fnmatch_escaped_literal(pattern, pattern_index);
                eval_fnmatch_literal(filename, flags, filename_index, literal)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        next_pattern_index,
                        filename_index + 1,
                        memo,
                    )
            }
            literal => {
                eval_fnmatch_literal(filename, flags, filename_index, literal)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        pattern_index + 1,
                        filename_index + 1,
                        memo,
                    )
            }
        }
    };
    memo[pattern_index][filename_index] = Some(result);
    result
}

/// Handles `*`, including pathname and leading-period restrictions.
pub(in crate::interpreter) fn eval_fnmatch_star(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    let mut next_pattern_index = pattern_index + 1;
    while next_pattern_index < pattern.len() && pattern[next_pattern_index] == b'*' {
        next_pattern_index += 1;
    }
    if eval_fnmatch_at(
        pattern,
        filename,
        flags,
        next_pattern_index,
        filename_index,
        memo,
    ) {
        return true;
    }
    let mut cursor = filename_index;
    while cursor < filename.len() && eval_fnmatch_wildcard_can_consume(filename, flags, cursor) {
        cursor += 1;
        if eval_fnmatch_at(pattern, filename, flags, next_pattern_index, cursor, memo) {
            return true;
        }
    }
    false
}

/// Returns whether `?` can consume the current filename byte.
pub(in crate::interpreter) fn eval_fnmatch_single_wildcard(
    filename: &[u8],
    flags: i64,
    filename_index: usize,
) -> bool {
    filename_index < filename.len()
        && eval_fnmatch_wildcard_can_consume(filename, flags, filename_index)
}

/// Handles a bracket class, or falls back to a literal `[` when the class is malformed.
pub(in crate::interpreter) fn eval_fnmatch_class_or_literal(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if filename_index >= filename.len()
        || !eval_fnmatch_wildcard_can_consume(filename, flags, filename_index)
    {
        return false;
    }
    let Some((matches, next_pattern_index)) =
        eval_fnmatch_class_matches(pattern, pattern_index + 1, filename[filename_index], flags)
    else {
        return eval_fnmatch_literal(filename, flags, filename_index, b'[')
            && eval_fnmatch_at(
                pattern,
                filename,
                flags,
                pattern_index + 1,
                filename_index + 1,
                memo,
            );
    };
    matches
        && eval_fnmatch_at(
            pattern,
            filename,
            flags,
            next_pattern_index,
            filename_index + 1,
            memo,
        )
}

/// Matches one bracket class body against the current filename byte.
pub(in crate::interpreter) fn eval_fnmatch_class_matches(
    pattern: &[u8],
    mut index: usize,
    candidate: u8,
    flags: i64,
) -> Option<(bool, usize)> {
    let negated = matches!(pattern.get(index).copied(), Some(b'!' | b'^'));
    if negated {
        index += 1;
    }
    let mut matched = false;
    let mut closed = false;
    while index < pattern.len() {
        if pattern[index] == b']' {
            closed = true;
            index += 1;
            break;
        }
        let start = eval_fnmatch_class_char(pattern, &mut index, flags)?;
        if index + 1 < pattern.len() && pattern[index] == b'-' && pattern[index + 1] != b']' {
            index += 1;
            let end = eval_fnmatch_class_char(pattern, &mut index, flags)?;
            if eval_fnmatch_byte_in_range(candidate, start, end, flags) {
                matched = true;
            }
        } else if eval_fnmatch_byte_eq(candidate, start, flags) {
            matched = true;
        }
    }
    closed.then_some((if negated { !matched } else { matched }, index))
}

/// Reads one character from a bracket class, respecting escapes when enabled.
pub(in crate::interpreter) fn eval_fnmatch_class_char(
    pattern: &[u8],
    index: &mut usize,
    flags: i64,
) -> Option<u8> {
    if *index >= pattern.len() {
        return None;
    }
    if pattern[*index] == b'\\' && flags & EVAL_FNM_NOESCAPE == 0 && *index + 1 < pattern.len() {
        *index += 2;
        return Some(pattern[*index - 1]);
    }
    let byte = pattern[*index];
    *index += 1;
    Some(byte)
}

/// Returns whether one candidate byte falls within a possibly case-folded range.
pub(in crate::interpreter) fn eval_fnmatch_byte_in_range(
    candidate: u8,
    start: u8,
    end: u8,
    flags: i64,
) -> bool {
    let candidate = eval_fnmatch_fold(candidate, flags);
    let start = eval_fnmatch_fold(start, flags);
    let end = eval_fnmatch_fold(end, flags);
    if start <= end {
        candidate >= start && candidate <= end
    } else {
        candidate >= end && candidate <= start
    }
}

/// Reads an escaped literal token outside bracket classes.
pub(in crate::interpreter) fn eval_fnmatch_escaped_literal(
    pattern: &[u8],
    pattern_index: usize,
) -> (u8, usize) {
    if pattern_index + 1 < pattern.len() {
        (pattern[pattern_index + 1], pattern_index + 2)
    } else {
        (b'\\', pattern_index + 1)
    }
}

/// Returns whether one literal pattern byte matches the current filename byte.
pub(in crate::interpreter) fn eval_fnmatch_literal(
    filename: &[u8],
    flags: i64,
    filename_index: usize,
    literal: u8,
) -> bool {
    filename_index < filename.len()
        && eval_fnmatch_byte_eq(filename[filename_index], literal, flags)
}

/// Returns whether a wildcard token may consume the current filename byte.
pub(in crate::interpreter) fn eval_fnmatch_wildcard_can_consume(
    filename: &[u8],
    flags: i64,
    filename_index: usize,
) -> bool {
    if filename_index >= filename.len() {
        return false;
    }
    if flags & EVAL_FNM_PATHNAME != 0 && filename[filename_index] == b'/' {
        return false;
    }
    if flags & EVAL_FNM_PERIOD != 0
        && eval_fnmatch_is_leading_period(filename, flags, filename_index)
    {
        return false;
    }
    true
}

/// Returns whether the current byte is a leading period for `FNM_PERIOD`.
pub(in crate::interpreter) fn eval_fnmatch_is_leading_period(
    filename: &[u8],
    flags: i64,
    filename_index: usize,
) -> bool {
    filename[filename_index] == b'.'
        && (filename_index == 0
            || (flags & EVAL_FNM_PATHNAME != 0 && filename[filename_index - 1] == b'/'))
}

/// Compares bytes using ASCII case folding when `FNM_CASEFOLD` is present.
pub(in crate::interpreter) fn eval_fnmatch_byte_eq(left: u8, right: u8, flags: i64) -> bool {
    eval_fnmatch_fold(left, flags) == eval_fnmatch_fold(right, flags)
}

/// Applies eval fnmatch's ASCII case folding.
pub(in crate::interpreter) fn eval_fnmatch_fold(byte: u8, flags: i64) -> u8 {
    if flags & EVAL_FNM_CASEFOLD != 0 {
        byte.to_ascii_lowercase()
    } else {
        byte
    }
}
