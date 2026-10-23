//! Purpose:
//! Declarative eval registry entry and implementation for `version_compare`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors php-src's `php_version_compare` and the compiled `__rt_version_compare` helper:
//!   C-string semantics (stop at NUL), canonicalization unless the version starts with `#`,
//!   trailing separators dropped, numeric tokens compared with saturating `strtol`, other
//!   tokens by special-form order (prefix matched), and the remaining tokens of the longer
//!   version compared against the `#` form.
//! - Without an operator (or with `null`) the result is the int -1/0/1; with one it is the
//!   bool predicate, and an unknown operator raises php-src's `ValueError`.

eval_builtin! {
    contract: "version_compare",
    area: String,
    direct: VersionCompare,
    values: VersionCompare,
}

use super::super::super::*;

/// php-src's `ValueError` wording for an unknown operator.
const VERSION_COMPARE_OPERATOR_MESSAGE: &str =
    "version_compare(): Argument #3 ($operator) must be a valid comparison operator";

/// Special version forms and their order, matched by prefix in this order.
const SPECIAL_FORMS: &[(&[u8], i64)] = &[
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

/// Evaluates PHP `version_compare(...)` over eval expressions.
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
    eval_version_compare_values_result(&evaluated, context, values)
}

/// Dispatches already evaluated `version_compare()` arguments.
pub(in crate::interpreter) fn eval_version_compare_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (first, second, operator) = match args {
        [first, second] => (*first, *second, None),
        [first, second, operator] => (*first, *second, Some(*operator)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let first = values.string_bytes(first)?;
    let second = values.string_bytes(second)?;
    let compare = eval_version_compare(&first, &second);
    let operator = match operator {
        Some(operator) if !values.is_null(operator)? => values.string_bytes(operator)?,
        _ => return values.int(compare),
    };
    let result = match operator.as_slice() {
        b"<" | b"lt" => compare == -1,
        b"<=" | b"le" => compare != 1,
        b">" | b"gt" => compare == 1,
        b">=" | b"ge" => compare != -1,
        b"==" | b"=" | b"eq" => compare == 0,
        b"!=" | b"<>" | b"ne" => compare != 0,
        _ => {
            let exception = values.new_object("ValueError")?;
            let message = values.string(VERSION_COMPARE_OPERATOR_MESSAGE)?;
            let code = values.int(0)?;
            values.construct_object(exception, vec![message, code])?;
            context.set_pending_throw(exception);
            return Err(EvalStatus::UncaughtThrowable);
        }
    };
    values.bool_value(result)
}

/// Compares two versions the way php-src's `php_version_compare()` does.
pub(in crate::interpreter) fn eval_version_compare(first: &[u8], second: &[u8]) -> i64 {
    let first = c_string(first);
    let second = c_string(second);
    if first.is_empty() {
        return if second.is_empty() { 0 } else { -1 };
    }
    if second.is_empty() {
        return 1;
    }
    let first = prepare(first);
    let second = prepare(second);
    compare_prepared(&first, &second)
}

/// Truncates a PHP string at its first NUL, as the C implementation reads it.
fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len())]
}

/// Canonicalizes a version unless it starts with `#`, dropping trailing separators.
fn prepare(version: &[u8]) -> Vec<u8> {
    if version[0] == b'#' {
        return version.to_vec();
    }
    let mut output = canonicalize(version);
    while output.last() == Some(&b'.') {
        output.pop();
    }
    output
}

/// php-src's `php_canonicalize_version()`.
fn canonicalize(version: &[u8]) -> Vec<u8> {
    let is_digit = |byte: u8| byte.is_ascii_digit();
    let is_non_digit = |byte: u8| !byte.is_ascii_digit() && byte != b'.';
    let mut output = Vec::with_capacity(version.len() * 2);
    output.push(version[0]);
    let mut previous = version[0];
    for &byte in &version[1..] {
        let last = *output.last().unwrap_or(&0);
        if matches!(byte, b'-' | b'_' | b'+') {
            if last != b'.' {
                output.push(b'.');
            }
        } else if (is_non_digit(previous) && is_digit(byte)) || (is_digit(previous) && is_non_digit(byte)) {
            if last != b'.' {
                output.push(b'.');
            }
            output.push(byte);
        } else if !byte.is_ascii_alphanumeric() {
            if last != b'.' {
                output.push(b'.');
            }
        } else {
            output.push(byte);
        }
        previous = byte;
    }
    output
}

/// Orders one non-numeric token by php-src's special forms (prefix match), -1 otherwise.
fn special_form(token: &[u8]) -> i64 {
    SPECIAL_FORMS
        .iter()
        .find(|(name, _)| token.starts_with(name))
        .map_or(-1, |(_, order)| *order)
}

/// `strtol()` over a token's leading digits, saturating at `PHP_INT_MAX`.
fn token_number(token: &[u8]) -> i64 {
    token
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0i64, |value, byte| value.saturating_mul(10).saturating_add(i64::from(byte - b'0')))
}

/// Splits the next `.`-delimited token off `rest`, reporting whether a separator followed.
fn next_token<'a>(rest: &mut &'a [u8]) -> (&'a [u8], bool) {
    match rest.iter().position(|&byte| byte == b'.') {
        Some(dot) => {
            let token = &rest[..dot];
            *rest = &rest[dot + 1..];
            (token, true)
        }
        None => (*rest, false),
    }
}

/// Compares two prepared (canonical or verbatim `#`) versions.
fn compare_prepared(first: &[u8], second: &[u8]) -> i64 {
    let starts_with_digit = |token: &[u8]| token.first().is_some_and(u8::is_ascii_digit);
    let (mut rest1, mut rest2) = (first, second);
    let (mut more1, mut more2) = (true, true);
    while !rest1.is_empty() && !rest2.is_empty() && more1 && more2 {
        let (token1, has1) = next_token(&mut rest1);
        let (token2, has2) = next_token(&mut rest2);
        more1 = has1;
        more2 = has2;
        let compare = match (starts_with_digit(token1), starts_with_digit(token2)) {
            (true, true) => token_number(token1).cmp(&token_number(token2)),
            (false, false) => special_form(token1).cmp(&special_form(token2)),
            (true, false) => 4.cmp(&special_form(token2)),
            (false, true) => special_form(token1).cmp(&4),
        };
        if compare != std::cmp::Ordering::Equal {
            return compare as i64;
        }
    }
    if more1 {
        compare_rest(rest1, 1)
    } else if more2 {
        compare_rest(rest2, -1)
    } else {
        0
    }
}

/// Compares the remaining tokens of one version against the `#` form; `side` is 1 when the
/// remaining version is the first argument and -1 when it is the second.
fn compare_rest(mut rest: &[u8], side: i64) -> i64 {
    loop {
        if rest.is_empty() {
            return -side;
        }
        let (token, more) = next_token(&mut rest);
        if token.first().is_some_and(u8::is_ascii_digit) {
            return side;
        }
        let compare = special_form(token).cmp(&4) as i64 * side;
        if compare != 0 {
            return compare;
        }
        if !more {
            return 0;
        }
    }
}
