//! Purpose:
//! Turns one staged argument block into a call on the crate's Rust API and writes the
//! outcome back into a result block.
//!
//! Called from:
//! - `crate::abi::elephc_iconv_call`, which wraps this in panic protection.
//!
//! Key details:
//! - Every failure is converted here into the diagnostic line php-src prints plus the
//!   PHP `false` result, so the generated runtime never formats messages itself.
//! - `iconv_strpos()`'s out-of-range `$offset` is the one outcome that must throw rather
//!   than warn, and it is reported through a dedicated result kind.
//! - Operation names are needed only to render diagnostics; dispatch itself is by opcode.

use crate::abi::args::{
    IconvCallArgs, OP_CONVERT, OP_GET_ENCODING, OP_MIME_DECODE, OP_MIME_DECODE_HEADERS,
    OP_MIME_ENCODE, OP_SET_ENCODING, OP_STRLEN, OP_STRPOS, OP_STRRPOS, OP_SUBSTR,
};
use crate::abi::result::{
    pack_entries, IconvResultBlock, KIND_ARRAY, KIND_INT, KIND_OFFSET_VALUE_ERROR, KIND_STRING,
    KIND_TRUE,
};
use crate::encoding_state::{self, EncodingKind};
use crate::error::{IconvError, IconvResult};
use crate::mime::encode::{MimeEncodeOptions, Scheme, DEFAULT_LINE_BREAK, DEFAULT_LINE_LENGTH};
use crate::{convert, mime, search};

/// Runs one staged iconv operation and fills in its result block.
///
/// # Safety
/// `args` and `out` must point at a valid argument block and a reset result block, and
/// every present string slot must describe a readable byte range.
pub unsafe fn dispatch(args: &IconvCallArgs, out: *mut IconvResultBlock) {
    match args.op {
        OP_CONVERT => {
            let value = convert::convert(
                args.bytes_or_empty(0),
                args.bytes_or_empty(1),
                args.bytes_or_empty(2),
            );
            finish_bytes("iconv", value, out);
        }
        OP_STRLEN => {
            let value = search::strlen(args.bytes_or_empty(0), args.bytes(1));
            finish_int("iconv_strlen", value.map(|len| len as i64), out);
        }
        OP_SUBSTR => {
            let value = search::substr(
                args.bytes_or_empty(0),
                args.int_or(1, 0),
                args.int(2),
                args.bytes(3),
            );
            finish_bytes("iconv_substr", value, out);
        }
        OP_STRPOS => {
            let value = search::strpos(
                args.bytes_or_empty(0),
                args.bytes_or_empty(1),
                args.int_or(2, 0),
                args.bytes(3),
            );
            finish_search("iconv_strpos", value, out);
        }
        OP_STRRPOS => {
            let value = search::strrpos(
                args.bytes_or_empty(0),
                args.bytes_or_empty(1),
                args.bytes(2),
            );
            finish_search("iconv_strrpos", value, out);
        }
        OP_MIME_ENCODE => {
            let options = mime_encode_options(args);
            let value = mime::encode::mime_encode(
                args.bytes_or_empty(0),
                args.bytes_or_empty(1),
                &options,
            );
            finish_bytes("iconv_mime_encode", value, out);
        }
        OP_MIME_DECODE => {
            let value = mime::decode::mime_decode(
                args.bytes_or_empty(0),
                args.int_or(1, 0),
                args.bytes(2),
            );
            finish_bytes("iconv_mime_decode", value, out);
        }
        OP_MIME_DECODE_HEADERS => {
            let value = mime::decode::mime_decode_headers(
                args.bytes_or_empty(0),
                args.int_or(1, 0),
                args.bytes(2),
            );
            match value {
                Ok(entries) => {
                    IconvResultBlock::set_bytes(out, KIND_ARRAY, pack_entries(&entries));
                }
                Err(error) => report(out, "iconv_mime_decode_headers", &error),
            }
        }
        OP_GET_ENCODING => get_encoding(args, out),
        OP_SET_ENCODING => set_encoding(args, out),
        // An unknown opcode can only come from a miscompiled call site; report PHP false.
        _ => {}
    }
}

/// Implements `iconv_get_encoding()`, which reports either one slot or the whole trio.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn get_encoding(args: &IconvCallArgs, out: *mut IconvResultBlock) {
    let requested = args.bytes(0).unwrap_or(b"all");
    if requested.eq_ignore_ascii_case(b"all") {
        let entries: Vec<(Vec<u8>, Vec<Vec<u8>>)> = EncodingKind::all()
            .into_iter()
            .map(|kind| {
                (
                    kind.key().as_bytes().to_vec(),
                    vec![encoding_state::get(kind).into_bytes()],
                )
            })
            .collect();
        IconvResultBlock::set_bytes(out, KIND_ARRAY, pack_entries(&entries));
        return;
    }
    if let Some(kind) = EncodingKind::parse(requested) {
        IconvResultBlock::set_bytes(out, KIND_STRING, encoding_state::get(kind).into_bytes());
    }
}

/// Implements `iconv_set_encoding()`, which php-src accepts without validating the value.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn set_encoding(args: &IconvCallArgs, out: *mut IconvResultBlock) {
    let Some(kind) = EncodingKind::parse(args.bytes_or_empty(0)) else {
        return;
    };
    if encoding_state::set(kind, args.bytes_or_empty(1)) {
        (*out).kind = KIND_TRUE;
    }
}

/// Builds the `iconv_mime_encode()` option set from its staged slots.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn mime_encode_options(args: &IconvCallArgs) -> MimeEncodeOptions {
    let mut options = MimeEncodeOptions::default();
    if let Some(scheme) = args.bytes(2) {
        options.scheme = Scheme::parse(scheme);
    }
    if let Some(charset) = args.bytes(3) {
        options.output_charset = charset.to_vec();
    }
    if let Some(charset) = args.bytes(4) {
        options.input_charset = charset.to_vec();
    }
    options.line_length = args.int_or(5, DEFAULT_LINE_LENGTH);
    options.line_break = match args.bytes(6) {
        Some(breaks) => breaks.to_vec(),
        None => DEFAULT_LINE_BREAK.to_vec(),
    };
    options
}

/// Records a byte-string result or the diagnostic that replaces it.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn finish_bytes(function: &str, value: IconvResult<Vec<u8>>, out: *mut IconvResultBlock) {
    match value {
        Ok(bytes) => IconvResultBlock::set_bytes(out, KIND_STRING, bytes),
        Err(error) => report(out, function, &error),
    }
}

/// Records an integer result or the diagnostic that replaces it.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn finish_int(function: &str, value: IconvResult<i64>, out: *mut IconvResultBlock) {
    match value {
        Ok(number) => {
            (*out).kind = KIND_INT;
            (*out).int_value = number;
        }
        Err(error) => report(out, function, &error),
    }
}

/// Records a search result, distinguishing "not found" from a thrown `ValueError`.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn finish_search(
    function: &str,
    value: Result<Option<usize>, search::SearchFailure>,
    out: *mut IconvResultBlock,
) {
    match value {
        Ok(Some(position)) => {
            (*out).kind = KIND_INT;
            (*out).int_value = position as i64;
        }
        // PHP reports "no match" as `false` without any diagnostic.
        Ok(None) => {}
        Err(search::SearchFailure::Conversion(error)) => report(out, function, &error),
        Err(search::SearchFailure::OffsetOutOfRange) => {
            (*out).kind = KIND_OFFSET_VALUE_ERROR;
        }
    }
}

/// Attaches the php-src diagnostic line for one failure, leaving the result as `false`.
///
/// # Safety
/// Same requirements as [`dispatch`].
unsafe fn report(out: *mut IconvResultBlock, function: &str, error: &IconvError) {
    IconvResultBlock::set_diagnostic(out, error.diagnostic_line(function));
}
