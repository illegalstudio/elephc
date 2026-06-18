//! Purpose:
//! PCRE-style preg builtins and regex capture helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Evaluates PHP `preg_match()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_match(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_match_result(pattern, subject, values)
        }
        [pattern, subject, matches] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let (result, matches_array) =
                eval_preg_match_capture_result(pattern, subject, None, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        [pattern, subject, matches, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let flags = eval_expr(flags, context, scope, values)?;
            let (result, matches_array) =
                eval_preg_match_capture_result(pattern, subject, Some(flags), values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns whether one regex matches the subject string.
pub(in crate::interpreter) fn eval_preg_match_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    values.int(i64::from(regex.is_match(&subject)))
}

/// Returns the match flag plus PHP `$matches` capture array for one regex search.
pub(in crate::interpreter) fn eval_preg_match_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let flags = eval_preg_match_flags(flags, values)?;
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    if let Some(captures) = regex.captures(&subject) {
        let matches = eval_preg_capture_array(
            &subject,
            Some(&captures),
            offset_capture,
            unmatched_as_null,
            values,
        )?;
        let matched = values.int(1)?;
        return Ok((matched, matches));
    }
    let matches =
        eval_preg_capture_array(&subject, None, offset_capture, unmatched_as_null, values)?;
    let matched = values.int(0)?;
    Ok((matched, matches))
}

/// Returns supported `preg_match()` flags.
pub(in crate::interpreter) fn eval_preg_match_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    let supported = EVAL_PREG_OFFSET_CAPTURE | EVAL_PREG_UNMATCHED_AS_NULL;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Evaluates PHP `preg_match_all()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_match_all(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_match_all_result(pattern, subject, values)
        }
        [pattern, subject, matches] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let (result, matches_array) =
                eval_preg_match_all_capture_result(pattern, subject, None, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        [pattern, subject, matches, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let EvalExpr::LoadVar(matches_name) = matches else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let flags = eval_expr(flags, context, scope, values)?;
            let (result, matches_array) =
                eval_preg_match_all_capture_result(pattern, subject, Some(flags), values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                matches_name.clone(),
                matches_array,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(result)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Counts all non-overlapping regex matches in one subject string.
pub(in crate::interpreter) fn eval_preg_match_all_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let count = regex.captures_iter(&subject).count();
    values.int(i64::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Returns the match count plus PHP's default `PREG_PATTERN_ORDER` `$matches` array.
pub(in crate::interpreter) fn eval_preg_match_all_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let capture_count = regex.captures_len();
    let subject = values.string_bytes(subject)?;
    let captures: Vec<Captures<'_>> = regex.captures_iter(&subject).collect();
    let count = values.int(i64::try_from(captures.len()).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let flags = eval_preg_match_all_flags(flags, values)?;
    let matches = if flags & EVAL_PREG_SET_ORDER != 0 {
        eval_preg_match_all_set_order_array(&subject, &captures, capture_count, flags, values)?
    } else {
        eval_preg_match_all_pattern_order_array(&subject, &captures, capture_count, flags, values)?
    };
    Ok((count, matches))
}

/// Returns supported `preg_match_all()` flags.
pub(in crate::interpreter) fn eval_preg_match_all_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(EVAL_PREG_PATTERN_ORDER);
    };
    let flags = eval_int_value(flags, values)?;
    let supported = EVAL_PREG_PATTERN_ORDER
        | EVAL_PREG_SET_ORDER
        | EVAL_PREG_OFFSET_CAPTURE
        | EVAL_PREG_UNMATCHED_AS_NULL;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Builds PHP's default `preg_match_all()` pattern-order capture matrix.
pub(in crate::interpreter) fn eval_preg_match_all_pattern_order_array(
    subject: &[u8],
    captures: &[Captures<'_>],
    capture_count: usize,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut outer = values.array_new(capture_count)?;
    for capture_index in 0..capture_count {
        let mut row = values.array_new(captures.len())?;
        for (match_index, capture) in captures.iter().enumerate() {
            let key =
                values.int(i64::try_from(match_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                capture,
                capture_index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            row = values.array_set(row, key, value)?;
        }
        let key =
            values.int(i64::try_from(capture_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        outer = values.array_set(outer, key, row)?;
    }
    Ok(outer)
}

/// Builds PHP's `preg_match_all(..., PREG_SET_ORDER)` match-order capture matrix.
pub(in crate::interpreter) fn eval_preg_match_all_set_order_array(
    subject: &[u8],
    captures: &[Captures<'_>],
    capture_count: usize,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut outer = values.array_new(captures.len())?;
    for (match_index, capture) in captures.iter().enumerate() {
        let mut row = values.array_new(capture_count)?;
        for capture_index in 0..capture_count {
            let key =
                values.int(i64::try_from(capture_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                capture,
                capture_index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            row = values.array_set(row, key, value)?;
        }
        let key = values.int(i64::try_from(match_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        outer = values.array_set(outer, key, row)?;
    }
    Ok(outer)
}

/// Evaluates PHP `preg_replace()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, replacement, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    let replacement = eval_expr(replacement, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_preg_replace_result(pattern, replacement, subject, values)
}

/// Replaces every regex match with a PHP-style backreference-expanded replacement.
pub(in crate::interpreter) fn eval_preg_replace_result(
    pattern: RuntimeCellHandle,
    replacement: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let replacement = values.string_bytes(replacement)?;
    let subject = values.string_bytes(subject)?;
    let mut result = Vec::with_capacity(subject.len());
    let mut cursor = 0;
    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.extend_from_slice(&subject[cursor..matched.start()]);
        eval_preg_expand_replacement(&replacement, &subject, &captures, &mut result);
        cursor = matched.end();
    }
    result.extend_from_slice(&subject[cursor..]);
    values.string_bytes_value(&result)
}

/// Evaluates PHP `preg_replace_callback()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_replace_callback(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, callback, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    let callback = eval_expr(callback, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_preg_replace_callback_result(pattern, callback, subject, context, values)
}

/// Replaces every regex match by invoking an eval-supported callback with `$matches`.
pub(in crate::interpreter) fn eval_preg_replace_callback_result(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let callback = eval_callable_name(callback, values)?;
    let subject = values.string_bytes(subject)?;
    let mut result = Vec::with_capacity(subject.len());
    let mut cursor = 0;
    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.extend_from_slice(&subject[cursor..matched.start()]);
        let matches = eval_preg_capture_array(&subject, Some(&captures), false, false, values)?;
        let callback_result = eval_callable_with_values(&callback, vec![matches], context, values)?;
        let callback_result = values.cast_string(callback_result)?;
        let callback_bytes = values.string_bytes(callback_result)?;
        result.extend_from_slice(&callback_bytes);
        cursor = matched.end();
    }
    result.extend_from_slice(&subject[cursor..]);
    values.string_bytes_value(&result)
}

/// Evaluates PHP `preg_split()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_split_result(pattern, subject, None, None, values)
        }
        [pattern, subject, limit] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let limit = eval_expr(limit, context, scope, values)?;
            eval_preg_split_result(pattern, subject, Some(limit), None, values)
        }
        [pattern, subject, limit, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let limit = eval_expr(limit, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_preg_split_result(pattern, subject, Some(limit), Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits a subject string with eval-supported `preg_split()` flags.
pub(in crate::interpreter) fn eval_preg_split_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let limit = eval_preg_split_limit(limit, values)?;
    let flags = eval_preg_split_flags(flags, values)?;
    let no_empty = flags & EVAL_PREG_SPLIT_NO_EMPTY != 0;
    let capture_delimiters = flags & EVAL_PREG_SPLIT_DELIM_CAPTURE != 0;
    let offset_capture = flags & EVAL_PREG_SPLIT_OFFSET_CAPTURE != 0;
    let mut pieces = Vec::<EvalPregSplitPiece>::new();
    let mut cursor = 0;

    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        if eval_preg_split_reached_limit(&pieces, limit) {
            break;
        }
        eval_preg_split_push_piece(
            &mut pieces,
            &subject[cursor..matched.start()],
            cursor,
            no_empty,
        );
        if capture_delimiters {
            eval_preg_split_push_captures(&mut pieces, &subject, &captures, no_empty);
        }
        cursor = matched.end();
    }
    eval_preg_split_push_piece(&mut pieces, &subject[cursor..], cursor, no_empty);

    let mut result = values.array_new(pieces.len())?;
    for (index, piece) in pieces.iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = eval_preg_split_piece_value(piece, offset_capture, values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Compiles one eval PCRE-style delimited pattern into a Rust regex.
pub(in crate::interpreter) fn eval_preg_regex(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Regex, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    let (body, modifiers) = eval_preg_pattern_parts(&pattern)?;
    let body = String::from_utf8(body).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut builder = RegexBuilder::new(&body);
    builder
        .case_insensitive(modifiers.case_insensitive)
        .multi_line(modifiers.multi_line)
        .dot_matches_new_line(modifiers.dot_matches_new_line)
        .swap_greed(modifiers.swap_greed);
    builder.build().map_err(|_| EvalStatus::RuntimeFatal)
}

/// Regex modifiers supported by eval `preg_*` pattern stripping.
#[derive(Default)]
pub(in crate::interpreter) struct EvalPregModifiers {
    case_insensitive: bool,
    multi_line: bool,
    dot_matches_new_line: bool,
    swap_greed: bool,
}

/// One `preg_split()` output segment plus its byte offset in the subject.
pub(in crate::interpreter) struct EvalPregSplitPiece {
    bytes: Vec<u8>,
    offset: usize,
}

/// Splits a PHP delimited regex into body bytes and supported modifiers.
pub(in crate::interpreter) fn eval_preg_pattern_parts(
    pattern: &[u8],
) -> Result<(Vec<u8>, EvalPregModifiers), EvalStatus> {
    if pattern.len() < 2 || pattern[0].is_ascii_alphanumeric() || pattern[0].is_ascii_whitespace() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let delimiter = pattern[0];
    if delimiter == b'\\' {
        return Err(EvalStatus::RuntimeFatal);
    }
    let closing = eval_preg_closing_delimiter(delimiter);
    let close_index =
        eval_preg_find_closing_delimiter(pattern, closing).ok_or(EvalStatus::RuntimeFatal)?;
    let body = eval_preg_unescape_delimiter(&pattern[1..close_index], delimiter, closing);
    let modifiers = eval_preg_modifiers(&pattern[close_index + 1..])?;
    Ok((body, modifiers))
}

/// Returns the closing regex delimiter for PHP's paired delimiter forms.
pub(in crate::interpreter) fn eval_preg_closing_delimiter(delimiter: u8) -> u8 {
    match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => delimiter,
    }
}

/// Finds the first unescaped closing regex delimiter.
pub(in crate::interpreter) fn eval_preg_find_closing_delimiter(
    pattern: &[u8],
    closing: u8,
) -> Option<usize> {
    let mut escaped = false;
    for (index, byte) in pattern.iter().copied().enumerate().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == closing {
            return Some(index);
        }
    }
    None
}

/// Removes escapes that only protect the PHP regex delimiter from pattern stripping.
pub(in crate::interpreter) fn eval_preg_unescape_delimiter(
    body: &[u8],
    delimiter: u8,
    closing: u8,
) -> Vec<u8> {
    let mut result = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        if body[index] == b'\\'
            && index + 1 < body.len()
            && matches!(body[index + 1], byte if byte == delimiter || byte == closing)
        {
            result.push(body[index + 1]);
            index += 2;
        } else {
            result.push(body[index]);
            index += 1;
        }
    }
    result
}

/// Parses eval-supported PHP regex modifiers.
pub(in crate::interpreter) fn eval_preg_modifiers(
    modifiers: &[u8],
) -> Result<EvalPregModifiers, EvalStatus> {
    let mut parsed = EvalPregModifiers::default();
    for modifier in modifiers {
        match *modifier {
            b'i' => parsed.case_insensitive = true,
            b'm' => parsed.multi_line = true,
            b's' => parsed.dot_matches_new_line = true,
            b'U' => parsed.swap_greed = true,
            b'u' => {}
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(parsed)
}

/// Builds PHP's indexed `$matches` capture array for one regex result.
pub(in crate::interpreter) fn eval_preg_capture_array(
    subject: &[u8],
    captures: Option<&Captures<'_>>,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = captures.map_or(0, |captures| {
        eval_preg_visible_capture_len(captures, unmatched_as_null)
    });
    let mut result = values.array_new(len)?;
    if let Some(captures) = captures {
        for index in 0..len {
            let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                captures,
                index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Returns the capture count PHP should expose, dropping trailing unmatched groups.
pub(in crate::interpreter) fn eval_preg_visible_capture_len(
    captures: &Captures<'_>,
    unmatched_as_null: bool,
) -> usize {
    if unmatched_as_null {
        return captures.len();
    }
    let mut len = captures.len();
    while len > 1 && captures.get(len - 1).is_none() {
        len -= 1;
    }
    len
}

/// Returns one captured byte range from the original subject.
pub(in crate::interpreter) fn eval_preg_capture_bytes<'a>(
    subject: &'a [u8],
    captures: &Captures<'_>,
    index: usize,
) -> Option<&'a [u8]> {
    captures
        .get(index)
        .map(|matched| &subject[matched.start()..matched.end()])
}

/// Builds one capture entry as either a string or PHP's `[string, byte_offset]` pair.
pub(in crate::interpreter) fn eval_preg_capture_value(
    subject: &[u8],
    captures: &Captures<'_>,
    index: usize,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let matched = captures.get(index);
    let value = if matched.is_none() && unmatched_as_null {
        values.null()?
    } else {
        let bytes = matched.as_ref().map_or(b"".as_slice(), |matched| {
            &subject[matched.start()..matched.end()]
        });
        values.string_bytes_value(bytes)?
    };
    if !offset_capture {
        return Ok(value);
    }

    let offset = matched.map_or(Ok(-1_i64), |matched| {
        i64::try_from(matched.start()).map_err(|_| EvalStatus::RuntimeFatal)
    })?;
    let offset = values.int(offset)?;
    let mut pair = values.array_new(2)?;
    let value_key = values.int(0)?;
    pair = values.array_set(pair, value_key, value)?;
    let offset_key = values.int(1)?;
    values.array_set(pair, offset_key, offset)
}

/// Appends one replacement string after expanding `$n`, `${n}`, and `\n` captures.
pub(in crate::interpreter) fn eval_preg_expand_replacement(
    replacement: &[u8],
    subject: &[u8],
    captures: &Captures<'_>,
    result: &mut Vec<u8>,
) {
    let mut index = 0;
    while index < replacement.len() {
        match replacement[index] {
            b'$' => {
                if let Some((capture_index, next_index)) =
                    eval_preg_replacement_capture_index(replacement, index + 1)
                {
                    if let Some(bytes) = eval_preg_capture_bytes(subject, captures, capture_index) {
                        result.extend_from_slice(bytes);
                    }
                    index = next_index;
                } else {
                    result.push(replacement[index]);
                    index += 1;
                }
            }
            b'\\' if index + 1 < replacement.len() && replacement[index + 1].is_ascii_digit() => {
                let (capture_index, next_index) =
                    eval_preg_decimal_capture_index(replacement, index + 1);
                if let Some(bytes) = eval_preg_capture_bytes(subject, captures, capture_index) {
                    result.extend_from_slice(bytes);
                }
                index = next_index;
            }
            byte => {
                result.push(byte);
                index += 1;
            }
        }
    }
}

/// Parses a dollar-style replacement capture reference.
pub(in crate::interpreter) fn eval_preg_replacement_capture_index(
    bytes: &[u8],
    index: usize,
) -> Option<(usize, usize)> {
    if bytes.get(index).copied() == Some(b'{') {
        let mut cursor = index + 1;
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == start || bytes.get(cursor).copied() != Some(b'}') {
            return None;
        }
        let capture = eval_preg_decimal_bytes_to_usize(&bytes[start..cursor])?;
        return Some((capture, cursor + 1));
    }
    if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        let (capture, next) = eval_preg_decimal_capture_index(bytes, index);
        return Some((capture, next));
    }
    None
}

/// Parses a one- or two-digit replacement capture reference.
pub(in crate::interpreter) fn eval_preg_decimal_capture_index(
    bytes: &[u8],
    index: usize,
) -> (usize, usize) {
    let mut cursor = index;
    let end = usize::min(bytes.len(), index + 2);
    while cursor < end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    (
        eval_preg_decimal_bytes_to_usize(&bytes[index..cursor]).unwrap_or(0),
        cursor,
    )
}

/// Converts ASCII decimal bytes into a `usize` capture index.
pub(in crate::interpreter) fn eval_preg_decimal_bytes_to_usize(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    for byte in bytes {
        value = value.checked_mul(10)?;
        value = value.checked_add(usize::from(byte - b'0'))?;
    }
    Some(value)
}

/// Returns the PHP `preg_split()` limit, treating zero as unlimited.
pub(in crate::interpreter) fn eval_preg_split_limit(
    limit: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let Some(limit) = limit else {
        return Ok(None);
    };
    let limit = eval_int_value(limit, values)?;
    if limit <= 0 {
        return Ok(None);
    }
    usize::try_from(limit)
        .map(Some)
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns supported `preg_split()` flags.
pub(in crate::interpreter) fn eval_preg_split_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    let supported =
        EVAL_PREG_SPLIT_NO_EMPTY | EVAL_PREG_SPLIT_DELIM_CAPTURE | EVAL_PREG_SPLIT_OFFSET_CAPTURE;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Returns whether `preg_split()` should stop splitting and emit the remaining subject.
pub(in crate::interpreter) fn eval_preg_split_reached_limit(
    pieces: &[EvalPregSplitPiece],
    limit: Option<usize>,
) -> bool {
    matches!(limit, Some(limit) if limit > 0 && pieces.len() + 1 >= limit)
}

/// Pushes one `preg_split()` output piece, honoring `PREG_SPLIT_NO_EMPTY`.
pub(in crate::interpreter) fn eval_preg_split_push_piece(
    pieces: &mut Vec<EvalPregSplitPiece>,
    piece: &[u8],
    offset: usize,
    no_empty: bool,
) {
    if no_empty && piece.is_empty() {
        return;
    }
    pieces.push(EvalPregSplitPiece {
        bytes: piece.to_vec(),
        offset,
    });
}

/// Pushes captured delimiters for `PREG_SPLIT_DELIM_CAPTURE`.
pub(in crate::interpreter) fn eval_preg_split_push_captures(
    pieces: &mut Vec<EvalPregSplitPiece>,
    subject: &[u8],
    captures: &Captures<'_>,
    no_empty: bool,
) {
    for index in 1..captures.len() {
        if let Some(matched) = captures.get(index) {
            eval_preg_split_push_piece(
                pieces,
                &subject[matched.start()..matched.end()],
                matched.start(),
                no_empty,
            );
        }
    }
}

/// Converts one split segment to a string or PHP `[string, byte_offset]` pair.
pub(in crate::interpreter) fn eval_preg_split_piece_value(
    piece: &EvalPregSplitPiece,
    offset_capture: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.string_bytes_value(&piece.bytes)?;
    if !offset_capture {
        return Ok(value);
    }

    let offset = i64::try_from(piece.offset).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = values.int(offset)?;
    let mut pair = values.array_new(2)?;
    let value_key = values.int(0)?;
    pair = values.array_set(pair, value_key, value)?;
    let offset_key = values.int(1)?;
    values.array_set(pair, offset_key, offset)
}
