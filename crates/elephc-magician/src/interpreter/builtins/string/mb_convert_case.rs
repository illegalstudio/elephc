//! Purpose:
//! Declarative eval registry entry and PHP 8.5 UTF-8 implementation for `mb_convert_case()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - Signature matches PHP: `mb_convert_case(string $string, int $mode, ?string $encoding = null)`.
//! - Title case uses Unicode `Cased` / `Case_Ignorable` plus Greek final-sigma rules.
//! - Omitted/null/`UTF-8` encodings convert UTF-8; `8bit`/`binary`/`7bit` treat bytes as `U+00xx`;
//!   other names decode through libc iconv and raise catchable `ValueError` when rejected.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};

use super::super::super::*;
use super::mb_convert_case_ignorable::CASE_IGNORABLE_RANGES;

eval_builtin! {
    contract: "mb_convert_case",
    area: String,
    direct: MbConvertCase,
    values: MbConvertCase,
}

const MB_CASE_UPPER: i64 = 0;
const MB_CASE_LOWER: i64 = 1;
const MB_CASE_TITLE: i64 = 2;
const MB_CASE_FOLD: i64 = 3;
const MB_CASE_UPPER_SIMPLE: i64 = 4;
const MB_CASE_LOWER_SIMPLE: i64 = 5;
const MB_CASE_TITLE_SIMPLE: i64 = 6;
const MB_CASE_FOLD_SIMPLE: i64 = 7;

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

/// Evaluates direct `mb_convert_case()` calls in PHP source-argument order.
pub(in crate::interpreter) fn eval_builtin_mb_convert_case(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, mode] => {
            let value = eval_expr(value, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_mb_convert_case_result(value, mode, None, context, values)
        }
        [value, mode, encoding] => {
            let value = eval_expr(value, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            let encoding = eval_expr(encoding, context, scope, values)?;
            eval_mb_convert_case_result(value, mode, Some(encoding), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts one materialized string with a PHP case mode and optional encoding.
pub(in crate::interpreter) fn eval_mb_convert_case_result(
    value: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    encoding: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mode = eval_int_value(mode, values)?;
    if !(MB_CASE_UPPER..=MB_CASE_FOLD_SIMPLE).contains(&mode) {
        return eval_mb_convert_case_mode_error(context, values);
    }
    let encoding = match encoding {
        Some(encoding) if !values.is_null(encoding)? => Some(values.string_bytes(encoding)?),
        _ => None,
    };
    let converted = match encoding.as_deref() {
        None => convert_utf8(&bytes, mode),
        Some(encoding) if is_utf8_name(encoding) => convert_utf8(&bytes, mode),
        Some(encoding) if is_byte_encoding_name(encoding) => convert_latin1_bytes(&bytes, mode),
        Some(encoding) => match convert_via_iconv(&bytes, encoding, mode) {
            Some(converted) => converted,
            None => return eval_mb_convert_case_encoding_error(encoding, context, values),
        },
    };
    values.string_bytes_value(&converted)
}

/// Returns whether `name` is PHP's default UTF-8 encoding alias.
fn is_utf8_name(name: &[u8]) -> bool {
    name.eq_ignore_ascii_case(b"UTF-8") || name.eq_ignore_ascii_case(b"UTF8")
}

/// Returns whether `name` is PHP's one-byte identity encoding alias.
fn is_byte_encoding_name(name: &[u8]) -> bool {
    name.eq_ignore_ascii_case(b"8bit")
        || name.eq_ignore_ascii_case(b"binary")
        || name.eq_ignore_ascii_case(b"7bit")
}

/// Converts through iconv into UTF-8, applies Unicode case mapping, and converts back.
fn convert_via_iconv(bytes: &[u8], encoding: &[u8], mode: i64) -> Option<Vec<u8>> {
    let utf8 = iconv_convert(bytes, encoding, b"UTF-8")?;
    let converted = convert_utf8(&utf8, mode);
    iconv_convert(&converted, b"UTF-8", encoding)
}

/// Converts `bytes` from `from` to `to` with libc iconv, grouping invalid bytes like mbstring.
fn iconv_convert(bytes: &[u8], from: &[u8], to: &[u8]) -> Option<Vec<u8>> {
    let from = CString::new(from).ok()?;
    let to = CString::new(to).ok()?;
    let descriptor = unsafe { iconv_open(to.as_ptr(), from.as_ptr()) };
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
        output.push(unsafe { *input as u8 });
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

/// Raises PHP's catchable `ValueError` for a `$mode` outside `MB_CASE_*`.
fn eval_mb_convert_case_mode_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    throw_value_error(
        context,
        values,
        "mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants",
    )
}

/// Raises PHP's catchable `ValueError` for an encoding name rejected by the runtime.
fn eval_mb_convert_case_encoding_error<T>(
    encoding: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let encoding = String::from_utf8_lossy(encoding);
    throw_value_error(
        context,
        values,
        &format!(
            "mb_convert_case(): Argument #3 ($encoding) must be a valid encoding, \"{}\" given",
            encoding
        ),
    )
}

/// Constructs a pending `ValueError` and returns eval's uncaught-throwable status.
fn throw_value_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    message: &str,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Converts UTF-8 bytes with PHP 8.5 `mb_convert_case()` semantics.
fn convert_utf8(bytes: &[u8], mode: i64) -> Vec<u8> {
    apply_case_to_units(&decode_utf8_units(bytes), mode)
        .into_iter()
        .flat_map(encode_unit)
        .collect()
}

/// Converts 8-bit identity encodings by treating each byte as `U+00xx`.
fn convert_latin1_bytes(bytes: &[u8], mode: i64) -> Vec<u8> {
    let units: Vec<DecodedUnit> = bytes
        .iter()
        .map(|&b| DecodedUnit::Scalar(b as u32))
        .collect();
    apply_case_to_units(&units, mode)
        .into_iter()
        .flat_map(|code| {
            if code <= 0xFF {
                vec![code as u8]
            } else {
                encode_unit(code)
            }
        })
        .collect()
}

/// One decoded input unit: a Unicode scalar or a raw malformed byte slice.
enum DecodedUnit {
    Scalar(u32),
    Raw(Vec<u8>),
}

/// Returns whether `code` is Unicode `Case_Ignorable`.
fn is_case_ignorable(code: u32) -> bool {
    CASE_IGNORABLE_RANGES
        .binary_search_by(|&(lo, hi)| {
            if code < lo {
                std::cmp::Ordering::Greater
            } else if code > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Returns whether `code` is Unicode `Cased`.
fn is_cased(code: u32) -> bool {
    char::from_u32(code).is_some_and(|ch| ch.is_lowercase() || ch.is_uppercase())
}

/// Maps one code point with PHP full/simple case conversion.
fn map_codepoint(code: u32, mode: i64, title_mode: bool) -> Vec<u32> {
    let Some(ch) = char::from_u32(code) else {
        return vec![code];
    };
    match mode {
        MB_CASE_UPPER => collect_full(ch.to_uppercase(), code),
        MB_CASE_LOWER | MB_CASE_FOLD => collect_full(ch.to_lowercase(), code),
        MB_CASE_TITLE => {
            if title_mode {
                collect_full(ch.to_lowercase(), code)
            } else {
                collect_full(titlecase_chars(ch), code)
            }
        }
        MB_CASE_UPPER_SIMPLE => vec![simple_or_self(ch.to_uppercase(), code)],
        MB_CASE_LOWER_SIMPLE | MB_CASE_FOLD_SIMPLE => vec![simple_or_self(ch.to_lowercase(), code)],
        MB_CASE_TITLE_SIMPLE => {
            if title_mode {
                vec![simple_or_self(ch.to_lowercase(), code)]
            } else {
                vec![simple_or_self(titlecase_chars(ch), code)]
            }
        }
        _ => vec![code],
    }
}

/// Returns the Unicode titlecase mapping for `ch`.
fn titlecase_chars(ch: char) -> Vec<char> {
    if let Some(mapped) = digraph_title(ch) {
        return vec![mapped];
    }
    let upper: Vec<char> = ch.to_uppercase().collect();
    match upper.as_slice() {
        [] => vec![ch],
        [only] => vec![*only],
        [first, rest @ ..] => {
            let mut out = vec![*first];
            for next in rest {
                out.extend(next.to_lowercase());
            }
            out.truncate(3);
            out
        }
    }
}

/// Returns the Unicode titlecase letter for DZ/LJ/NJ digraphs.
fn digraph_title(ch: char) -> Option<char> {
    let mapped = match ch as u32 {
        0x01C4 | 0x01C5 | 0x01C6 => 0x01C5,
        0x01C7 | 0x01C8 | 0x01C9 => 0x01C8,
        0x01CA | 0x01CB | 0x01CC => 0x01CB,
        0x01F1 | 0x01F2 | 0x01F3 => 0x01F2,
        _ => return None,
    };
    char::from_u32(mapped)
}

/// Collects a full Unicode case mapping, falling back to `original` if empty.
fn collect_full<I>(iter: I, original: u32) -> Vec<u32>
where
    I: IntoIterator<Item = char>,
{
    let mapped: Vec<u32> = iter.into_iter().take(3).map(|ch| ch as u32).collect();
    if mapped.is_empty() {
        vec![original]
    } else {
        mapped
    }
}

/// Returns the single-code-point mapping, or `original` when the full map expands.
fn simple_or_self<I>(iter: I, original: u32) -> u32
where
    I: IntoIterator<Item = char>,
{
    let mapped: Vec<char> = iter.into_iter().collect();
    match mapped.as_slice() {
        [ch] => *ch as u32,
        _ => original,
    }
}

/// Decodes UTF-8 into scalars, grouping each malformed sequence as one raw unit.
fn decode_utf8_units(bytes: &[u8]) -> Vec<DecodedUnit> {
    let mut units = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                units.extend(valid.chars().map(|ch| DecodedUnit::Scalar(ch as u32)));
                break;
            }
            Err(error) => {
                let valid_len = error.valid_up_to();
                if valid_len > 0 {
                    let valid = std::str::from_utf8(&bytes[offset..offset + valid_len])
                        .expect("from_utf8 valid prefix");
                    units.extend(valid.chars().map(|ch| DecodedUnit::Scalar(ch as u32)));
                }
                match error.error_len() {
                    Some(invalid_len) => {
                        units.push(DecodedUnit::Raw(
                            bytes[offset + valid_len..offset + valid_len + invalid_len].to_vec(),
                        ));
                        offset += valid_len + invalid_len;
                    }
                    None => {
                        units.push(DecodedUnit::Raw(bytes[offset + valid_len..].to_vec()));
                        break;
                    }
                }
            }
        }
    }
    units
}

/// Applies PHP case conversion, including title-case state and final-sigma rules.
fn apply_case_to_units(units: &[DecodedUnit], mode: i64) -> Vec<u32> {
    let mut out = Vec::new();
    let mut title_mode = false;
    let full_lower_or_title = mode == MB_CASE_LOWER || mode == MB_CASE_TITLE;
    for (index, unit) in units.iter().enumerate() {
        match unit {
            DecodedUnit::Raw(raw) => {
                out.extend(raw.iter().map(|&b| b as u32));
                title_mode = false;
            }
            DecodedUnit::Scalar(code) => {
                if full_lower_or_title
                    && *code == 0x03A3
                    && (mode != MB_CASE_TITLE || title_mode)
                    && should_use_final_sigma(units, index)
                {
                    out.push(0x03C2);
                } else {
                    out.extend(map_codepoint(*code, mode, title_mode));
                }
                if matches!(mode, MB_CASE_TITLE | MB_CASE_TITLE_SIMPLE) && !is_case_ignorable(*code)
                {
                    title_mode = is_cased(*code);
                }
            }
        }
    }
    out
}

/// Implements PHP 8.3+ final-sigma: last cased letter in a word, ignoring Case_Ignorable.
fn should_use_final_sigma(units: &[DecodedUnit], index: usize) -> bool {
    let mut saw_cased_before = false;
    for unit in units[..index].iter().rev() {
        match unit {
            DecodedUnit::Raw(_) => break,
            DecodedUnit::Scalar(code) => {
                if is_case_ignorable(*code) {
                    continue;
                }
                saw_cased_before = is_cased(*code);
                break;
            }
        }
    }
    if !saw_cased_before {
        return false;
    }
    for unit in units[index + 1..].iter() {
        match unit {
            DecodedUnit::Raw(_) => return true,
            DecodedUnit::Scalar(code) => {
                if is_case_ignorable(*code) {
                    continue;
                }
                return !is_cased(*code);
            }
        }
    }
    true
}

/// Encodes one output code point as UTF-8 bytes, or as a raw byte when malformed.
fn encode_unit(code: u32) -> Vec<u8> {
    if let Some(ch) = char::from_u32(code) {
        let mut buf = [0u8; 4];
        ch.encode_utf8(&mut buf).as_bytes().to_vec()
    } else if code <= 0xFF {
        vec![code as u8]
    } else {
        vec![b'?']
    }
}
