//! Purpose:
//! Implements PHP's `iconv_mime_encode()`: it folds one header field into RFC 2047
//! encoded-words that respect the configured line length.
//!
//! Called from:
//! - `crate::abi::dispatch` and Magician's `iconv_mime_encode` binding.
//!
//! Key details:
//! - The line budget is tracked exactly as php-src does: a counter seeded with
//!   `line-length`, decremented by every byte written, and refilled with
//!   `line-length - 1` whenever a continuation line starts.
//! - `B` words size their conversion from the base64 expansion and grow the reserved
//!   tail when the closing shift sequence does not fit; `Q` words retry with a smaller
//!   conversion until the escaped form fits the remaining budget.
//! - A field name that cannot be represented in ASCII contributes nothing, because
//!   php-src's `_php_iconv_appendl` abandons its output before committing it.

use crate::encoding_state::{self, EncodingKind};
use crate::error::{IconvError, IconvResult};
use crate::ffi::Converter;
use crate::mime::{base64, quoted_printable};

/// Default `line-length` option value.
pub const DEFAULT_LINE_LENGTH: i64 = 76;

/// Default `line-break-chars` option value.
pub const DEFAULT_LINE_BREAK: &[u8] = b"\r\n";

/// Output bytes php-src initially reserves for a base64 word's closing shift sequence.
const BASE64_RESERVED: i64 = 4;

/// Transfer encoding selected by the `scheme` option.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheme {
    /// RFC 2047 `B` (base64) encoded-words.
    Base64,
    /// RFC 2047 `Q` (quoted-printable) encoded-words.
    QuotedPrintable,
}

impl Scheme {
    /// Selects a scheme from the PHP option string, keeping `B` for anything unknown.
    pub fn parse(value: &[u8]) -> Self {
        match value.first() {
            Some(b'Q') | Some(b'q') => Scheme::QuotedPrintable,
            _ => Scheme::Base64,
        }
    }

    /// Returns the single-letter tag written into the encoded-word header.
    fn tag(self) -> u8 {
        match self {
            Scheme::Base64 => b'B',
            Scheme::QuotedPrintable => b'Q',
        }
    }

    /// Returns the per-scheme slack php-src adds to the minimum encoded-word length.
    fn minimum_word_slack(self) -> i64 {
        match self {
            Scheme::Base64 => 4,
            Scheme::QuotedPrintable => 3,
        }
    }
}

/// Normalized `$options` array for `iconv_mime_encode()`.
pub struct MimeEncodeOptions {
    /// Transfer encoding of every emitted encoded-word.
    pub scheme: Scheme,
    /// Charset the header is encoded into.
    pub output_charset: Vec<u8>,
    /// Charset `$field_value` is currently in.
    pub input_charset: Vec<u8>,
    /// Maximum length of one output line.
    pub line_length: i64,
    /// Bytes written between folded lines.
    pub line_break: Vec<u8>,
}

impl Default for MimeEncodeOptions {
    /// Builds the option set php-src uses when `$options` is empty.
    ///
    /// Both charsets start at `iconv.internal_encoding`; supplying only `input-charset`
    /// therefore also changes the charset the header is encoded into.
    fn default() -> Self {
        let internal = encoding_state::get(EncodingKind::Internal).into_bytes();
        Self {
            scheme: Scheme::Base64,
            output_charset: internal.clone(),
            input_charset: internal,
            line_length: DEFAULT_LINE_LENGTH,
            line_break: DEFAULT_LINE_BREAK.to_vec(),
        }
    }
}

/// Encodes one header field into RFC 2047 encoded-words.
pub fn mime_encode(
    field_name: &[u8],
    field_value: &[u8],
    options: &MimeEncodeOptions,
) -> IconvResult<Vec<u8>> {
    let charset_len = options.output_charset.len() as i64;
    let max_line_len = options.line_length;
    if (field_name.len() as i64 + 2) >= max_line_len || (charset_len + 12) >= max_line_len {
        return Err(IconvError::TooBig);
    }

    let reported = |error: IconvError| {
        error.with_reported_charsets(
            &String::from_utf8_lossy(&options.input_charset),
            &String::from_utf8_lossy(&options.output_charset),
        )
    };
    let mut ascii = Converter::open(&options.input_charset, b"ASCII").map_err(reported)?;
    let mut body =
        Converter::open(&options.input_charset, &options.output_charset).map_err(reported)?;

    // php-src sizes its scratch buffer from the line length, so an encoded-word can
    // never need more raw bytes than this.
    let mut scratch = vec![0u8; (max_line_len as usize).saturating_mul(5).max(16)];
    let mut out = Vec::with_capacity(field_value.len() * 2 + 32);
    let mut budget = max_line_len;

    // php-src appends nothing when the field name is not representable in ASCII.
    if let Ok(converted) = ascii.convert_all(field_name) {
        out.extend_from_slice(&converted);
    }
    budget -= field_name.len() as i64;
    out.extend_from_slice(b": ");
    budget -= 2;

    let minimum_word = 7 + charset_len + options.scheme.minimum_word_slack();
    let mut input = field_value;
    loop {
        if budget < minimum_word + options.line_break.len() as i64 + 1 {
            out.extend_from_slice(&options.line_break);
            out.push(b' ');
            budget = max_line_len - 1;
        }
        out.extend_from_slice(b"=?");
        budget -= 2;
        out.extend_from_slice(&options.output_charset);
        budget -= charset_len;
        out.push(b'?');
        budget -= 1;
        out.push(options.scheme.tag());
        budget -= 1;
        out.push(b'?');
        budget -= 1;

        let word = match options.scheme {
            Scheme::Base64 => {
                encode_base64_word(&mut body, &mut input, budget, &mut scratch).map_err(reported)?
            }
            Scheme::QuotedPrintable => {
                encode_qprint_word(&mut body, &mut input, budget, &mut scratch).map_err(reported)?
            }
        };
        if budget < word.len() as i64 {
            return Err(IconvError::TooBig);
        }
        out.extend_from_slice(&word);
        budget -= word.len() as i64;

        out.extend_from_slice(b"?=");
        budget -= 2;

        if input.is_empty() {
            break;
        }
    }
    Ok(out)
}

/// Encodes the largest base64 word that still fits the remaining line budget.
///
/// The conversion budget is fixed by the base64 expansion ratio; only the tail reserved
/// for the closing shift sequence grows when the first attempt cannot flush.
fn encode_base64_word(
    body: &mut Converter,
    input: &mut &[u8],
    budget: i64,
    scratch: &mut [u8],
) -> IconvResult<Vec<u8>> {
    let out_size = (budget - 2) / 4 * 3;
    let mut reserved = BASE64_RESERVED;
    loop {
        if out_size <= reserved {
            return Err(IconvError::TooBig);
        }
        let capacity = (out_size - reserved) as usize;
        let mut attempt = *input;
        let before = attempt.len();
        let (produced, too_big) = body.convert_into(&mut attempt, &mut scratch[..capacity])?;
        if too_big && attempt.len() == before {
            return Err(IconvError::TooBig);
        }
        let (flushed, flush_too_big) =
            body.flush_into(&mut scratch[produced..out_size as usize])?;
        if !flush_too_big {
            *input = attempt;
            return Ok(base64::encode(&scratch[..produced + flushed]));
        }
        // The closing shift sequence did not fit; reserve more room and convert less.
        body.reset();
        reserved += 4;
    }
}

/// Encodes the largest quoted-printable word that still fits the remaining line budget.
fn encode_qprint_word(
    body: &mut Converter,
    input: &mut &[u8],
    budget: i64,
    scratch: &mut [u8],
) -> IconvResult<Vec<u8>> {
    let capacity = budget - 2;
    let mut out_size = capacity;
    while out_size > 0 {
        let mut attempt = *input;
        let before = attempt.len();
        let (produced, too_big) = body.convert_into(&mut attempt, &mut scratch[..out_size as usize])?;
        if too_big && attempt.len() == before {
            return Err(IconvError::TooBig);
        }
        let (flushed, _) = body.flush_into(&mut scratch[produced..out_size as usize])?;
        let encoded = &scratch[..produced + flushed];
        let required: i64 = encoded
            .iter()
            .map(|byte| quoted_printable::cost(*byte) as i64)
            .sum();
        if required <= capacity {
            let word = quoted_printable::encode(encoded);
            *input = attempt;
            body.reset();
            return Ok(word);
        }
        // Each escaped byte costs three output bytes, so the overflow divided by three
        // (rounded up) is how much shorter the next conversion attempt has to be.
        out_size -= (required - capacity + 2) / 3;
        body.reset();
    }
    Err(IconvError::TooBig)
}
