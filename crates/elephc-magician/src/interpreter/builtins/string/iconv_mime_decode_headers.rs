//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_mime_decode_headers()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - A field name that repeats collects every value into a PHP list, exactly as the
//!   native backend does.

eval_builtin! {
    contract: "iconv_mime_decode_headers",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_charset, eval_iconv_failure};

/// Applies PHP `iconv_mime_decode_headers(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_mime_decode_headers_result(
    subject: RuntimeCellHandle,
    mode: Option<RuntimeCellHandle>,
    encoding: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = values.string_bytes(subject)?;
    let mode = match mode {
        Some(mode) if !values.is_null(mode)? => eval_int_value(mode, values)?,
        _ => 0,
    };
    let charset = eval_iconv_charset(encoding, values)?;
    let entries = match elephc_iconv::mime_decode_headers(&subject, mode, charset.as_deref()) {
        Ok(entries) => entries,
        Err(error) => {
            return eval_iconv_failure("iconv_mime_decode_headers", &error, values);
        }
    };
    let mut headers = values.assoc_new(entries.len())?;
    for (name, field_values) in entries {
        let key = values.string_bytes_value(&name)?;
        let value = if field_values.len() == 1 {
            values.string_bytes_value(&field_values[0])?
        } else {
            let mut list = values.array_new(field_values.len())?;
            for (index, entry) in field_values.iter().enumerate() {
                let index = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
                let entry = values.string_bytes_value(entry)?;
                list = values.array_set(list, index, entry)?;
            }
            list
        };
        headers = values.array_set(headers, key, value)?;
    }
    Ok(headers)
}
