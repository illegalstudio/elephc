//! Purpose:
//! Eval registry entry and implementation for PHP's `setlocale`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Locale arrays and variadic candidates are tried in PHP source order.
//! - PHP's scalar coercions apply before calling libc, and the string `"0"`
//!   queries the active locale through a null libc locale pointer.

use std::ffi::{CStr, CString};

use super::super::super::*;

eval_builtin! {
    contract: "setlocale",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `setlocale($category, $locales, ...$rest)` in source order.
pub(in crate::interpreter) fn eval_builtin_setlocale(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_setlocale_result(&evaluated_args, values)
}

/// Applies PHP `setlocale()` to already evaluated category and candidate values.
pub(in crate::interpreter) fn eval_setlocale_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let category = eval_int_value(evaluated_args[0], values)?;
    let category = libc::c_int::try_from(category).map_err(|_| EvalStatus::RuntimeFatal)?;
    for candidate in &evaluated_args[1..] {
        if values.is_array_like(*candidate)? {
            let len = values.array_len(*candidate)?;
            for position in 0..len {
                let key = values.array_iter_key(*candidate, position)?;
                let value = values.array_get(*candidate, key)?;
                if let Some(locale) = eval_setlocale_candidate(category, value, values)? {
                    return values.string_bytes_value(&locale);
                }
            }
        } else if let Some(locale) = eval_setlocale_candidate(category, *candidate, values)? {
            return values.string_bytes_value(&locale);
        }
    }
    values.bool_value(false)
}

/// Tries one scalar-coerced locale candidate and copies libc's static result bytes.
fn eval_setlocale_candidate(
    category: libc::c_int,
    candidate: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    let bytes = values.string_bytes(candidate)?;
    let result = if bytes == b"0" {
        unsafe { libc::setlocale(category, std::ptr::null()) }
    } else {
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
        let locale = CString::new(&bytes[..end]).map_err(|_| EvalStatus::RuntimeFatal)?;
        unsafe { libc::setlocale(category, locale.as_ptr()) }
    };
    if result.is_null() {
        return Ok(None);
    }
    Ok(Some(
        unsafe { CStr::from_ptr(result) }.to_bytes().to_vec(),
    ))
}
