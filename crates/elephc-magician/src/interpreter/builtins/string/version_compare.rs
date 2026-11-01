//! Purpose:
//! Eval registry entry and implementation for `version_compare`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - A transcription of php-src 8.5's `ext/standard/versioning.c`, the same one the compiled
//!   side's AST prelude (`src/version_compare_prelude.rs`) transcribes: canonicalization (with
//!   8.5.10's dropped trailing `.`), the special-form prefix table, `strtol` segments and the
//!   `#N#` sentinel. The arguments are C strings to php, so each one ends at its first NUL.
//! - The compiled side cannot serve interpreted code: a compiler prelude is injected only where
//!   the compiled program names the function, and Symfony's password hasher factory calls it
//!   from interpreted code on every login.

use super::super::super::*;

eval_builtin! {
    contract: "version_compare",
    area: String,
    direct: VersionCompare,
    values: VersionCompare,
}

/// Evaluates PHP `version_compare(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_version_compare(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_version_compare_values(&evaluated, context, values)
}

/// Evaluates PHP `version_compare(...)` from already-evaluated argument cells.
pub(in crate::interpreter) fn eval_version_compare_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (version1, version2, operator) = match evaluated_args {
        [a, b] => (*a, *b, None),
        [a, b, op] => (*a, *b, Some(*op)),
        _ => {
            let message = if evaluated_args.len() < 2 {
                format!(
                    "version_compare() expects at least 2 arguments, {} given",
                    evaluated_args.len()
                )
            } else {
                format!(
                    "version_compare() expects at most 3 arguments, {} given",
                    evaluated_args.len()
                )
            };
            return eval_throw_argument_count_error(&message, context, values);
        }
    };
    let version1 = values.string_bytes(version1)?;
    let version2 = values.string_bytes(version2)?;
    let compare = php_version_compare(until_nul(&version1), until_nul(&version2));
    let operator = match operator {
        Some(operator) if !values.is_null(operator)? => values.string_bytes(operator)?,
        _ => return values.int(i64::from(compare)),
    };
    let answer = match operator.as_slice() {
        b"<" | b"lt" => compare == -1,
        b"<=" | b"le" => compare != 1,
        b">" | b"gt" => compare == 1,
        b">=" | b"ge" => compare != -1,
        b"==" | b"=" | b"eq" => compare == 0,
        b"!=" | b"<>" | b"ne" => compare != 0,
        _ => {
            return eval_throw_builtin_value_error(
                "version_compare(): Argument #3 ($operator) must be a valid comparison operator",
                context,
                values,
            )
        }
    };
    values.bool_value(answer)
}

/// The bytes before the first NUL, as php's C-string reading sees them.
fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|byte| *byte == 0)
        .map_or(bytes, |end| &bytes[..end])
}

/// `php_canonicalize_version`.
fn canonicalize(version: &[u8]) -> Vec<u8> {
    let Some((&first, rest)) = version.split_first() else {
        return Vec::new();
    };
    let is_digit = |byte: u8| byte.is_ascii_digit();
    let is_non_digit = |byte: u8| !byte.is_ascii_digit() && byte != b'.';
    let mut out = Vec::with_capacity(version.len() * 2);
    out.push(first);
    let mut previous = first;
    for &byte in rest {
        let last = *out.last().expect("canonical form starts non-empty");
        if matches!(byte, b'-' | b'_' | b'+') {
            if last != b'.' {
                out.push(b'.');
            }
        } else if (is_non_digit(previous) && is_digit(byte))
            || (is_digit(previous) && is_non_digit(byte))
        {
            if last != b'.' {
                out.push(b'.');
            }
            out.push(byte);
        } else if !byte.is_ascii_alphanumeric() {
            if last != b'.' {
                out.push(b'.');
            }
        } else {
            out.push(byte);
        }
        previous = byte;
    }
    // php 8.5.10 drops an empty last component (a trailing `.`).
    if out.last() == Some(&b'.') {
        out.pop();
    }
    out
}

/// `compare_special_version_forms`'s order for one form: its first matching prefix, or -1.
fn special_order(form: &[u8]) -> i32 {
    const FORMS: [(&[u8], i32); 10] = [
        (b"dev", 0),
        (b"alpha", 1),
        (b"a", 1),
        (b"beta", 2),
        (b"b", 2),
        (b"RC", 3),
        (b"rc", 3),
        (b"#", 4),
        (b"pl", 5),
        (b"p", 5),
    ];
    FORMS
        .iter()
        .find(|(name, _)| form.starts_with(name))
        .map_or(-1, |(_, order)| *order)
}

/// `strtol(segment, NULL, 10)` for a segment that starts with a digit, saturating like it.
fn leading_number(segment: &[u8]) -> i64 {
    let mut value: i64 = 0;
    for &byte in segment.iter().take_while(|byte| byte.is_ascii_digit()) {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(i64::from(byte - b'0')))
            .unwrap_or(i64::MAX);
    }
    value
}

/// `php_version_compare`: -1, 0 or 1.
fn php_version_compare(original1: &[u8], original2: &[u8]) -> i32 {
    if original1.is_empty() || original2.is_empty() {
        return match (original1.is_empty(), original2.is_empty()) {
            (true, true) => 0,
            (false, _) => 1,
            _ => -1,
        };
    }
    let version1 = if original1[0] == b'#' { original1.to_vec() } else { canonicalize(original1) };
    let version2 = if original2[0] == b'#' { original2.to_vec() } else { canonicalize(original2) };
    let digit_at = |bytes: &[u8], index: usize| bytes.get(index).is_some_and(u8::is_ascii_digit);
    let next_dot = |bytes: &[u8], from: usize| {
        bytes[from..].iter().position(|byte| *byte == b'.').map(|offset| from + offset)
    };
    let (mut p1, mut p2) = (0usize, 0usize);
    let (mut n1, mut n2) = (Some(0usize), Some(0usize));
    let mut compare = 0;
    while p1 < version1.len() && p2 < version2.len() && n1.is_some() && n2.is_some() {
        n1 = next_dot(&version1, p1);
        n2 = next_dot(&version2, p2);
        let segment1 = &version1[p1..n1.unwrap_or(version1.len())];
        let segment2 = &version2[p2..n2.unwrap_or(version2.len())];
        let digit1 = digit_at(segment1, 0);
        let digit2 = digit_at(segment2, 0);
        compare = match (digit1, digit2) {
            (true, true) => leading_number(segment1).cmp(&leading_number(segment2)) as i32,
            (false, false) => (special_order(segment1) - special_order(segment2)).signum(),
            (true, false) => (special_order(b"#N#") - special_order(segment2)).signum(),
            (false, true) => (special_order(segment1) - special_order(b"#N#")).signum(),
        };
        if compare != 0 {
            break;
        }
        if let Some(dot) = n1 {
            p1 = dot + 1;
        }
        if let Some(dot) = n2 {
            p2 = dot + 1;
        }
    }
    if compare == 0 {
        if n1.is_some() {
            compare = if digit_at(&version1, p1) {
                1
            } else {
                php_version_compare(version1.get(p1..).unwrap_or_default(), b"#N#")
            };
        } else if n2.is_some() {
            compare = if digit_at(&version2, p2) {
                -1
            } else {
                php_version_compare(b"#N#", version2.get(p2..).unwrap_or_default())
            };
        }
    }
    compare
}

#[cfg(test)]
mod tests {
    use super::php_version_compare;

    /// Pins answers php 8.5.10's `version_compare()` gives, trailing-dot rule included.
    #[test]
    fn version_compare_matches_php() {
        for (left, right, expected) in [
            ("1.0.14", "1.0.22", -1),
            ("5.10", "5.2", 1),
            ("1.0", "1.0.0", -1),
            ("1.0-dev", "1.0", -1),
            ("1.0alpha", "1.0beta", -1),
            ("1.0RC1", "1.0", -1),
            ("1.0pl1", "1.0", 1),
            ("1", "1.", 0),
            ("", "1", -1),
            ("1", "1.a", 1),
            ("99999999999999999999", "1", 1),
        ] {
            assert_eq!(
                php_version_compare(left.as_bytes(), right.as_bytes()),
                expected,
                "{left} vs {right}"
            );
        }
    }
}
