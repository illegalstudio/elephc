//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `mb_strwidth()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - The eval signature matches PHP's nullable optional `$encoding` parameter.
//! - UTF-8 malformed/truncated sequences use Rust's decoder boundaries like `mb_strlen`;
//!   each valid scalar uses PHP 8.5 East Asian Width, and substitutions add width 1.
//! - Other encodings decode through libc iconv into UTF-32LE, then apply the same widths.
//! - Unknown encoding names raise a catchable `ValueError` through eval's pending-throw state.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};

use elephc_builtin_contract::character_width;

use super::super::super::*;

eval_builtin! {
    contract: "mb_strwidth",
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

/// Evaluates direct `mb_strwidth()` calls while preserving PHP source-order argument evaluation.
pub(in crate::interpreter) fn eval_builtin_mb_strwidth(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_mb_strwidth_result(value, None, context, values)
        }
        [value, encoding] => {
            let value = eval_expr(value, context, scope, values)?;
            let encoding = eval_expr(encoding, context, scope, values)?;
            eval_mb_strwidth_result(value, Some(encoding), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Measures display width of one materialized eval string with PHP-compatible encoding selection.
pub(in crate::interpreter) fn eval_mb_strwidth_result(
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
    let width = match encoding.as_deref() {
        None => width_utf8(&bytes),
        Some(encoding) => match width_in_encoding(&bytes, encoding) {
            Some(width) => width,
            None => return eval_mb_strwidth_encoding_error(encoding, context, values),
        },
    };
    let width = i64::try_from(width).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(width)
}

/// Sums PHP East Asian Width for UTF-8, treating each malformed or truncated sequence as 1.
fn width_utf8(bytes: &[u8]) -> usize {
    let mut offset = 0usize;
    let mut width = 0usize;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                width += valid.chars().map(|ch| character_width(ch as u32) as usize).sum::<usize>();
                break;
            }
            Err(error) => {
                let valid_len = error.valid_up_to();
                let valid = std::str::from_utf8(&bytes[offset..offset + valid_len])
                    .expect("from_utf8 valid prefix");
                width += valid.chars().map(|ch| character_width(ch as u32) as usize).sum::<usize>();
                width += 1;
                match error.error_len() {
                    Some(invalid_len) => offset += valid_len + invalid_len,
                    None => break,
                }
            }
        }
    }
    width
}

/// Measures width for a PHP encoding name, returning `None` when iconv rejects the name.
fn width_in_encoding(bytes: &[u8], encoding: &[u8]) -> Option<usize> {
    if encoding.eq_ignore_ascii_case(b"UTF-8") || encoding.eq_ignore_ascii_case(b"UTF8") {
        return Some(width_utf8(bytes));
    }
    if encoding.eq_ignore_ascii_case(b"8bit")
        || encoding.eq_ignore_ascii_case(b"binary")
        || encoding.eq_ignore_ascii_case(b"7bit")
    {
        return Some(bytes.len());
    }
    width_with_iconv(bytes, encoding)
}

/// Decodes through libc iconv into UTF-32LE and sums East Asian Width of produced scalars.
fn width_with_iconv(bytes: &[u8], encoding: &[u8]) -> Option<usize> {
    let encoding = CString::new(encoding).ok()?;
    let utf32le = c"UTF-32LE";
    let descriptor = unsafe { iconv_open(utf32le.as_ptr(), encoding.as_ptr()) };
    if descriptor as isize == -1 {
        return None;
    }

    let mut input = bytes.as_ptr().cast_mut().cast::<c_char>();
    let mut input_left = bytes.len();
    let mut width = 0usize;
    while input_left > 0 {
        let mut output = [0u8; 64];
        let mut output_ptr = output.as_mut_ptr().cast::<c_char>();
        let mut output_left = output.len();
        let status = unsafe {
            iconv(
                descriptor,
                &mut input,
                &mut input_left,
                &mut output_ptr,
                &mut output_left,
            )
        };
        let produced = output.len() - output_left;
        for chunk in output[..produced].chunks_exact(4) {
            let codepoint = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            width = width.checked_add(character_width(codepoint) as usize)?;
        }
        if status != usize::MAX {
            continue;
        }

        let errno = std::io::Error::last_os_error().raw_os_error();
        if errno == Some(libc::E2BIG) {
            continue;
        }
        if errno == Some(libc::EINVAL) {
            width = width.checked_add(1)?;
            break;
        }
        if errno != Some(libc::EILSEQ) || input_left == 0 {
            unsafe { iconv_close(descriptor) };
            return None;
        }

        input = unsafe { input.add(1) };
        input_left -= 1;
        width = width.checked_add(1)?;
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
    Some(width)
}

/// Raises PHP's catchable `ValueError` for an encoding name rejected by the runtime.
fn eval_mb_strwidth_encoding_error<T>(
    encoding: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let encoding = String::from_utf8_lossy(encoding);
    let message = format!(
        "mb_strwidth(): Argument #2 ($encoding) must be a valid encoding, \"{}\" given",
        encoding
    );
    let exception = values.new_object("ValueError")?;
    let message = values.string(&message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
