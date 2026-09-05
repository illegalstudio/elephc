//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `mb_strtoupper()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - The eval signature matches PHP's nullable optional `$encoding` parameter.
//! - UTF-8 uses Unicode full case mapping (`ß` → `SS`); malformed/truncated sequences
//!   are copied through unchanged, matching PHP 8.5 mbstring.
//! - `8bit`/`binary`/`7bit` apply ASCII-only uppercase; other encodings go through
//!   libc iconv into UTF-32LE, then back.
//! - Unknown encoding names raise a catchable `ValueError` through eval's pending-throw state.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};

use super::super::super::*;

eval_builtin! {
    contract: "mb_strtoupper",
    area: String,
    direct: Strlen,
    values: Strlen,
}

#[cfg_attr(target_os = "macos", link(name = "iconv"))]
unsafe extern "C" {
    fn iconv_open(tocode: *const c_char, fromcode: *const c_char) -> *mut c_void;
    fn iconv(
        cd: *mut c_void,
        inbuf: *mut *mut c_char,
        inbytesleft: *mut usize,
        outbuf: *mut *mut c_char,
        outbytesleft: *mut usize,
    ) -> usize;
    fn iconv_close(cd: *mut c_void) -> i32;
}

/// Evaluates direct `mb_strtoupper()` calls while preserving PHP source-order argument evaluation.
pub(in crate::interpreter) fn eval_builtin_mb_strtoupper(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_mb_strtoupper_result(value, None, context, values)
        }
        [value, encoding] => {
            let value = eval_expr(value, context, scope, values)?;
            let encoding = eval_expr(encoding, context, scope, values)?;
            eval_mb_strtoupper_result(value, Some(encoding), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Uppercases one materialized eval string with PHP-compatible encoding selection.
pub(in crate::interpreter) fn eval_mb_strtoupper_result(
    value: RuntimeCellHandle,
    encoding: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let encoding = match encoding {
        Some(encoding) if !values.is_null(encoding)? => Some(values.string_bytes(encoding)?),
        _ => None,
    };
    let upper = match encoding.as_deref() {
        None => utf8_strtoupper(&bytes),
        Some(encoding) => match strtoupper_in_encoding(&bytes, encoding) {
            Some(upper) => upper,
            None => return eval_mb_strtoupper_encoding_error(encoding, context, values),
        },
    };
    values.string_bytes_value(&upper)
}

/// Applies Unicode full uppercase to valid UTF-8 and copies malformed bytes through.
fn utf8_strtoupper(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut offset = 0usize;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                append_uppercase_chars(&mut output, valid);
                break;
            }
            Err(error) => {
                let valid_len = error.valid_up_to();
                if valid_len > 0 {
                    let valid = std::str::from_utf8(&bytes[offset..offset + valid_len])
                        .expect("from_utf8 valid prefix");
                    append_uppercase_chars(&mut output, valid);
                }
                match error.error_len() {
                    Some(invalid_len) => {
                        output.extend_from_slice(&bytes[offset + valid_len..offset + valid_len + invalid_len]);
                        offset += valid_len + invalid_len;
                    }
                    None => {
                        output.extend_from_slice(&bytes[offset + valid_len..]);
                        break;
                    }
                }
            }
        }
    }
    output
}

/// Appends Unicode full uppercase mappings for each scalar in `valid`.
fn append_uppercase_chars(output: &mut Vec<u8>, valid: &str) {
    for ch in valid.chars() {
        for upper in ch.to_uppercase() {
            let mut buf = [0u8; 4];
            output.extend_from_slice(upper.encode_utf8(&mut buf).as_bytes());
        }
    }
}

/// Uppercases `bytes` for a PHP encoding name, returning `None` when iconv rejects the name.
fn strtoupper_in_encoding(bytes: &[u8], encoding: &[u8]) -> Option<Vec<u8>> {
    if encoding.eq_ignore_ascii_case(b"UTF-8") || encoding.eq_ignore_ascii_case(b"UTF8") {
        return Some(utf8_strtoupper(bytes));
    }
    if encoding.eq_ignore_ascii_case(b"8bit")
        || encoding.eq_ignore_ascii_case(b"binary")
        || encoding.eq_ignore_ascii_case(b"7bit")
    {
        return Some(ascii_strtoupper(bytes));
    }
    iconv_strtoupper(bytes, encoding)
}

/// Applies ASCII-only uppercase, leaving every non `a-z` byte unchanged.
fn ascii_strtoupper(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_lowercase() {
                byte - (b'a' - b'A')
            } else {
                *byte
            }
        })
        .collect()
}

/// Decodes through libc iconv into UTF-32LE, applies Unicode uppercase, and encodes back.
fn iconv_strtoupper(bytes: &[u8], encoding: &[u8]) -> Option<Vec<u8>> {
    let utf32 = convert_with_iconv(bytes, encoding, b"UTF-32LE")?;
    let upper = uppercase_utf32le(&utf32);
    convert_with_iconv(&upper, b"UTF-32LE", encoding)
}

/// Uppercases UTF-32LE code units with Unicode full case mapping, skipping a leading BOM.
fn uppercase_utf32le(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut saw_payload = false;
    for chunk in bytes.chunks_exact(4) {
        let cp = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !saw_payload && cp == 0xFEFF {
            continue;
        }
        saw_payload = true;
        if let Some(ch) = char::from_u32(cp) {
            for upper in ch.to_uppercase() {
                output.extend_from_slice(&(upper as u32).to_le_bytes());
            }
        } else {
            output.extend_from_slice(chunk);
        }
    }
    output
}

/// Converts `bytes` from `from_encoding` to `to_encoding` through libc iconv.
fn convert_with_iconv(bytes: &[u8], from_encoding: &[u8], to_encoding: &[u8]) -> Option<Vec<u8>> {
    let from_encoding = CString::new(from_encoding).ok()?;
    let to_encoding = CString::new(to_encoding).ok()?;
    let descriptor = unsafe { iconv_open(to_encoding.as_ptr(), from_encoding.as_ptr()) };
    if descriptor as isize == -1 {
        return None;
    }

    let mut input = bytes.as_ptr().cast_mut().cast::<c_char>();
    let mut input_left = bytes.len();
    let mut output = Vec::new();
    while input_left > 0 {
        let mut scratch = [0u8; 256];
        let mut output_ptr = scratch.as_mut_ptr().cast::<c_char>();
        let mut output_left = scratch.len();
        let status = unsafe {
            iconv(
                descriptor,
                &mut input,
                &mut input_left,
                &mut output_ptr,
                &mut output_left,
            )
        };
        output.extend_from_slice(&scratch[..scratch.len() - output_left]);
        if status != usize::MAX {
            continue;
        }

        let errno = std::io::Error::last_os_error().raw_os_error();
        if errno == Some(libc::E2BIG) {
            continue;
        }
        if errno == Some(libc::EINVAL) {
            break;
        }
        if errno != Some(libc::EILSEQ) || input_left == 0 {
            unsafe { iconv_close(descriptor) };
            return None;
        }

        input = unsafe { input.add(1) };
        input_left -= 1;
        unsafe {
            iconv(
                descriptor,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
    }

    unsafe { iconv_close(descriptor) };
    Some(output)
}

/// Raises PHP's catchable `ValueError` for an encoding name rejected by the runtime.
fn eval_mb_strtoupper_encoding_error<T>(
    encoding: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let encoding = String::from_utf8_lossy(encoding);
    let message = format!(
        "mb_strtoupper(): Argument #2 ($encoding) must be a valid encoding, \"{}\" given",
        encoding
    );
    let exception = values.new_object("ValueError")?;
    let message = values.string(&message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
