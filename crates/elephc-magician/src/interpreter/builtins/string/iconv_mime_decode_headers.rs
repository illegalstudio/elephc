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

use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
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
    let mut headers = EvalArrayBuilder::assoc(values, entries.len())?;
    for (name, field_values) in entries {
        headers.entry_from_key(
            |values| values.string_bytes_value(&name),
            |values, _| {
                if let [entry] = field_values.as_slice() {
                    return values.string_bytes_value(entry);
                }
                let mut list = EvalArrayBuilder::indexed(values, field_values.len())?;
                for (index, entry) in field_values.iter().enumerate() {
                    list.index(index, |values| values.string_bytes_value(entry))?;
                }
                Ok(list.finish())
            },
        )?;
    }
    Ok(headers.finish())
}
