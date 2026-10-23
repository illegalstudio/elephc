//! Purpose:
//! Declarative eval registry entry and implementation for `substr_compare`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors php-src: a negative `$offset` counts from the end and clamps at zero, an
//!   `$offset` past the end or a negative `$length` is a `ValueError`, and `$length === 0`
//!   answers 0 before anything is compared.
//! - The result is the first differing byte difference (`memcmp` / ASCII-lowercased bytes),
//!   or the three-way comparison of the compared lengths when one is a prefix of the other.

eval_builtin! {
    contract: "substr_compare",
    area: String,
    direct: SubstrCompare,
    values: SubstrCompare,
}

use super::super::super::*;

/// Evaluates PHP `substr_compare(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_substr_compare(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_substr_compare_values_result(&evaluated, context, values)
}

/// Dispatches already evaluated `substr_compare()` arguments.
pub(in crate::interpreter) fn eval_substr_compare_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (haystack, needle, offset, rest) = match args {
        [haystack, needle, offset, rest @ ..] if rest.len() <= 2 => (*haystack, *needle, *offset, rest),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    eval_reject_container_argument("substr_compare", 3, "offset", "int", offset, context, values)?;
    let offset = eval_int_value(offset, values)?;
    let length = match rest.first() {
        Some(length) if !values.is_null(*length)? => {
            eval_reject_container_argument(
                "substr_compare",
                4,
                "length",
                "?int",
                *length,
                context,
                values,
            )?;
            Some(eval_int_value(*length, values)?)
        }
        _ => None,
    };
    let case_insensitive = match rest.get(1) {
        Some(flag) => values.truthy(*flag)?,
        None => false,
    };
    match eval_substr_compare(&haystack, &needle, offset, length, case_insensitive) {
        Ok(result) => values.int(result),
        Err(message) => eval_substr_compare_value_error(message, context, values),
    }
}

/// Computes php-src's `substr_compare()` result, or the `ValueError` message.
pub(in crate::interpreter) fn eval_substr_compare(
    haystack: &[u8],
    needle: &[u8],
    offset: i64,
    length: Option<i64>,
    case_insensitive: bool,
) -> Result<i64, &'static str> {
    if let Some(length) = length {
        if length < 0 {
            return Err("substr_compare(): Argument #4 ($length) must be greater than or equal to 0");
        }
        if length == 0 {
            return Ok(0);
        }
    }
    let total = haystack.len() as i64;
    let start = if offset < 0 { (total + offset).max(0) } else { offset };
    if start > total {
        return Err("substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)");
    }
    let main = &haystack[start as usize..];
    let compare_len = match length {
        Some(length) => length as usize,
        None => needle.len().max(main.len()),
    };
    let left = &main[..main.len().min(compare_len)];
    let right = &needle[..needle.len().min(compare_len)];
    let fold = |byte: u8| if case_insensitive { byte.to_ascii_lowercase() } else { byte };
    for (a, b) in left.iter().zip(right.iter()) {
        let (a, b) = (fold(*a), fold(*b));
        if a != b {
            return Ok(i64::from(a) - i64::from(b));
        }
    }
    Ok(match left.len().cmp(&right.len()) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// Raises PHP's catchable `ValueError` for an invalid `$offset` or `$length`.
fn eval_substr_compare_value_error<T>(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
