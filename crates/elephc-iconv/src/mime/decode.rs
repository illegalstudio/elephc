//! Purpose:
//! Implements PHP's `iconv_mime_decode()` and `iconv_mime_decode_headers()` as a literal
//! port of php-src's `_php_iconv_mime_decode` scanner.
//!
//! Called from:
//! - `crate::abi` for the AOT runtime and Magician's matching eval bindings.
//!
//! Key details:
//! - The scanner states are php-src's numbered states, kept one-to-one so the many
//!   non-RFC-compliant fallbacks (bare `CR`, undelimited encoded-words, unknown charsets
//!   under `CONTINUE_ON_ERROR`) behave identically.
//! - One call decodes exactly one header line and reports where the next one starts,
//!   which is what lets the headers variant walk a whole block field by field.
//! - Literal header text is converted from ASCII, and php-src deliberately ignores that
//!   conversion's failures in every state except the initial one.

use crate::encoding_state::effective_charset;
use crate::error::{IconvError, IconvResult};
use crate::ffi::Converter;
use crate::mime::{base64, quoted_printable};

/// `ICONV_MIME_DECODE_STRICT`: encoded-words must be delimited by whitespace.
pub const MODE_STRICT: i64 = 1;

/// `ICONV_MIME_DECODE_CONTINUE_ON_ERROR`: keep undecodable text instead of failing.
pub const MODE_CONTINUE_ON_ERROR: i64 = 2;

/// Charset placeholder php-src prints as the source in MIME decoder diagnostics.
const REPORTED_SOURCE_CHARSET: &str = "???";

/// Longest charset name php-src copies out of an encoded-word.
const MAX_CHARSET_NAME: usize = 79;

/// One decoded header line plus where the next one starts.
struct DecodedLine {
    /// Decoded and re-encoded field text.
    text: Vec<u8>,
    /// Offset of the first byte after this header line.
    next: usize,
}

/// Decodes the first header line of `input`, PHP `iconv_mime_decode()` style.
pub fn mime_decode(input: &[u8], mode: i64, charset: Option<&[u8]>) -> IconvResult<Vec<u8>> {
    let resolved = effective_charset(charset);
    let reported = reported_charset(charset, &resolved);
    Ok(decode_line(input, &resolved, &reported, mode)?.text)
}

/// Decodes every header line of `input` into ordered name/value pairs.
///
/// A field name that repeats keeps every value, which PHP surfaces as a list instead of
/// a single string.
pub fn mime_decode_headers(
    input: &[u8],
    mode: i64,
    charset: Option<&[u8]>,
) -> IconvResult<Vec<(Vec<u8>, Vec<Vec<u8>>)>> {
    let resolved = effective_charset(charset);
    let reported = reported_charset(charset, &resolved);
    let mut headers: Vec<(Vec<u8>, Vec<Vec<u8>>)> = Vec::new();
    let mut offset = 0usize;
    while offset < input.len() {
        let line = decode_line(&input[offset..], &resolved, &reported, mode)?;
        if line.text.is_empty() {
            break;
        }
        if let Some((name, value)) = split_field(&line.text) {
            match headers.iter_mut().find(|(key, _)| key == &name) {
                Some((_, values)) => values.push(value),
                None => headers.push((name, vec![value])),
            }
        }
        if line.next == 0 {
            break;
        }
        offset += line.next;
    }
    Ok(headers)
}

/// Returns the charset spelling php-src puts in a MIME decoder diagnostic.
///
/// php-src hands the raw `$encoding` argument straight to the error formatter, so an
/// explicitly empty charset is echoed as an empty name; an omitted one is already the
/// internal charset by the time the formatter sees it.
fn reported_charset(explicit: Option<&[u8]>, resolved: &[u8]) -> String {
    match explicit {
        Some(charset) => String::from_utf8_lossy(charset).into_owned(),
        None => String::from_utf8_lossy(resolved).into_owned(),
    }
}

/// Splits one decoded header line at its first colon, dropping the value's leading blanks.
fn split_field(line: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let colon = line.iter().position(|byte| *byte == b':')?;
    let mut value = &line[colon + 1..];
    while matches!(value.first(), Some(b' ') | Some(b'\t')) {
        value = &value[1..];
    }
    Some((line[..colon].to_vec(), value.to_vec()))
}

/// php-src's scanner states, kept numbered so the port stays comparable to the original.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// 0: expecting any character.
    Any,
    /// 1: expecting the `?` that opens an encoded-word.
    OpenDelimiter,
    /// 2: reading the charset name.
    CharsetName,
    /// 3: expecting the transfer-encoding letter.
    Scheme,
    /// 4: expecting the `?` before the payload.
    PayloadDelimiter,
    /// 5: reading the payload.
    Payload,
    /// 6: expecting the `=` that closes the encoded-word.
    CloseDelimiter,
    /// 7: expecting the `\n` of a line break.
    LineFeed,
    /// 8: deciding whether the next line folds into this one.
    FoldCheck,
    /// 9: deciding what follows a completed encoded-word.
    AfterWord,
    /// 10: skipping an RFC 2231 language tag.
    LanguageTag,
    /// 11: accumulating a run of whitespace.
    Whitespace,
    /// 12: reading literal text that may not start an encoded-word in strict mode.
    LiteralWord,
}

/// Mutable state of one header-line scan.
struct LineScanner<'a> {
    input: &'a [u8],
    mode: i64,
    /// Converter for literal text, which php-src reads as ASCII.
    plain: Converter,
    /// Charset every decoded fragment is re-encoded into.
    charset: Vec<u8>,
    /// Charset spelling the diagnostics echo back.
    reported: String,
    out: Vec<u8>,
    state: State,
    /// Offset of the `=` that opened the current encoded-word candidate.
    encoded_word: Option<usize>,
    /// Offset where the current whitespace run started.
    spaces: Option<usize>,
    /// Bounds of the charset name inside the current candidate.
    charset_name: Option<usize>,
    /// Bounds of the payload inside the current candidate.
    payload: Option<(usize, usize)>,
    /// Whether the current candidate carries a base64 payload.
    base64_payload: bool,
    /// Converter opened for the current candidate's charset.
    word_converter: Option<Converter>,
}

/// Decodes one header line and reports where the next line begins.
fn decode_line(
    input: &[u8],
    charset: &[u8],
    reported: &str,
    mode: i64,
) -> IconvResult<DecodedLine> {
    let plain = Converter::open(b"ASCII", charset)
        .map_err(|error| error.with_reported_charsets(REPORTED_SOURCE_CHARSET, reported))?;
    let mut scanner = LineScanner {
        input,
        mode,
        plain,
        charset: charset.to_vec(),
        reported: reported.to_string(),
        out: Vec::with_capacity(input.len()),
        state: State::Any,
        encoded_word: None,
        spaces: None,
        charset_name: None,
        payload: None,
        base64_payload: true,
        word_converter: None,
    };
    let next = scanner.run()?;
    Ok(DecodedLine {
        text: std::mem::take(&mut scanner.out),
        next,
    })
}

impl LineScanner<'_> {
    /// Whether the caller asked for undecodable text to be preserved.
    fn lenient(&self) -> bool {
        self.mode & MODE_CONTINUE_ON_ERROR != 0
    }

    /// Whether the caller asked for RFC 2047's delimiter rules to be enforced.
    fn strict(&self) -> bool {
        self.mode & MODE_STRICT != 0
    }

    /// Returns the state php-src falls back to after emitting literal text.
    fn literal_state(&self) -> State {
        if self.strict() {
            State::LiteralWord
        } else {
            State::Any
        }
    }

    /// Walks the header line, returning the offset of the following line.
    ///
    /// The loop mirrors php-src's `for (str_left = str_nbytes; str_left > 0; str_left--, p1++)`:
    /// `index` is `p1` and a state handler may rewind it by one to re-examine a byte.
    fn run(&mut self) -> IconvResult<usize> {
        let mut index = 0usize;
        while index < self.input.len() {
            let byte = self.input[index];
            match self.state {
                State::Any => self.scan_any(byte, index)?,
                State::OpenDelimiter => index = self.scan_open_delimiter(byte, index)?,
                State::CharsetName => {
                    if let Some(next) = self.scan_charset_name(byte, index)? {
                        index = next;
                        continue;
                    }
                }
                State::Scheme => self.scan_scheme(byte, index)?,
                State::PayloadDelimiter => self.scan_payload_delimiter(byte, index)?,
                State::Payload => self.scan_payload(byte, index),
                State::CloseDelimiter | State::AfterWord => {
                    self.scan_word_boundary(byte, index)?
                }
                State::LineFeed => self.scan_line_feed(byte),
                State::FoldCheck => {
                    if self.scan_fold(byte) {
                        // php-src rewinds one byte, then the loop increment restores it,
                        // so the next header starts at the byte that ended this one.
                        return Ok(index);
                    }
                }
                State::LanguageTag => {
                    if byte == b'?' {
                        self.state = State::Scheme;
                    }
                }
                State::Whitespace => self.scan_whitespace(byte, index)?,
                State::LiteralWord => self.scan_literal_word(byte, index),
            }
            index += 1;
        }
        self.finish_line()?;
        Ok(self.input.len())
    }

    /// State 0: any character may start text, whitespace, a line break, or a word.
    fn scan_any(&mut self, byte: u8, index: usize) -> IconvResult<()> {
        match byte {
            b'\r' => self.state = State::LineFeed,
            b'\n' => self.state = State::FoldCheck,
            b'=' => {
                self.encoded_word = Some(index);
                self.state = State::OpenDelimiter;
            }
            b' ' | b'\t' => {
                self.spaces = Some(index);
                self.state = State::Whitespace;
            }
            _ => {
                // This is the one place php-src surfaces a literal-text conversion failure.
                if let Err(error) = self.append_plain(&[byte]) {
                    if !self.lenient() {
                        return Err(error);
                    }
                }
                self.encoded_word = None;
                if self.strict() {
                    self.state = State::LiteralWord;
                }
            }
        }
        Ok(())
    }

    /// State 1: without the `?`, the text was never an encoded-word.
    fn scan_open_delimiter(&mut self, byte: u8, index: usize) -> IconvResult<usize> {
        if byte == b'?' {
            self.charset_name = Some(index + 1);
            self.state = State::CharsetName;
            return Ok(index);
        }
        // php-src rewinds a line break so the fold logic still sees it.
        let end = if byte == b'\r' || byte == b'\n' {
            index
        } else {
            index + 1
        };
        let start = self.encoded_word.unwrap_or(index);
        self.append_literal_range(start, end)?;
        self.encoded_word = None;
        self.state = self.literal_state();
        Ok(end.saturating_sub(1))
    }

    /// State 2: read the charset name and open its converter once it is delimited.
    ///
    /// Returns the offset to resume scanning at when the candidate turned out to be
    /// literal text that must be re-examined from the byte that ended the name.
    fn scan_charset_name(&mut self, byte: u8, index: usize) -> IconvResult<Option<usize>> {
        match byte {
            b'?' => self.state = State::Scheme,
            b'*' => self.state = State::LanguageTag,
            b'\r' | b'\n' => {
                let start = self.charset_name.take().unwrap_or(index);
                // php-src ignores the two delimiter bytes' conversion outcome.
                let _ = self.append_plain(b"=?");
                self.append_literal_range(start, index)?;
                self.state = self.literal_state();
                return Ok(Some(index));
            }
            _ => return Ok(None),
        }
        let Some(start) = self.charset_name else {
            return Err(IconvError::MalformedString);
        };
        let name = &self.input[start..index];
        if name.len() > MAX_CHARSET_NAME {
            return self.reject_word(index + 1).map(|()| None);
        }
        match Converter::open(name, &self.charset) {
            Ok(converter) => {
                self.word_converter = Some(converter);
                Ok(None)
            }
            Err(error) => {
                if !self.lenient() {
                    return Err(self.charset_error(error));
                }
                // php-src skips to the end of the word and keeps it verbatim.
                let end = self.skip_unknown_word(index);
                let start = self.encoded_word.unwrap_or(index);
                self.append_literal_range(start, end)?;
                self.encoded_word = None;
                self.state = State::LiteralWord;
                Ok(Some(end))
            }
        }
    }

    /// Finds the end of an encoded-word whose charset could not be opened.
    ///
    /// php-src walks forward over the two remaining `?` delimiters and takes the closing
    /// `=` when it is actually there.
    fn skip_unknown_word(&self, index: usize) -> usize {
        let mut cursor = index;
        let mut delimiters = 2;
        while delimiters > 0 && cursor + 1 < self.input.len() {
            cursor += 1;
            if self.input[cursor] == b'?' {
                delimiters -= 1;
            }
        }
        if self.input.get(cursor + 1) == Some(&b'=') {
            cursor += 1;
        }
        cursor + 1
    }

    /// State 3: the transfer-encoding letter selects base64 or quoted-printable.
    fn scan_scheme(&mut self, byte: u8, index: usize) -> IconvResult<()> {
        match byte {
            b'b' | b'B' => {
                self.base64_payload = true;
                self.state = State::PayloadDelimiter;
                Ok(())
            }
            b'q' | b'Q' => {
                self.base64_payload = false;
                self.state = State::PayloadDelimiter;
                Ok(())
            }
            _ => self.reject_word(index + 1),
        }
    }

    /// State 4: the payload must be introduced by a `?`.
    fn scan_payload_delimiter(&mut self, byte: u8, index: usize) -> IconvResult<()> {
        if byte != b'?' {
            return self.reject_word(index + 1);
        }
        self.payload = Some((index + 1, index + 1));
        self.state = State::Payload;
        Ok(())
    }

    /// State 5: the payload runs until the next `?`.
    fn scan_payload(&mut self, byte: u8, index: usize) {
        if byte == b'?' {
            if let Some((start, _)) = self.payload {
                self.payload = Some((start, index));
            }
            self.state = State::CloseDelimiter;
        }
    }

    /// States 6 and 9: close the encoded-word, then decide what its successor is.
    fn scan_word_boundary(&mut self, byte: u8, index: usize) -> IconvResult<()> {
        if self.state == State::CloseDelimiter {
            if byte != b'=' {
                return self.reject_word(index + 1);
            }
            self.state = State::AfterWord;
            if index + 1 != self.input.len() {
                return Ok(());
            }
            // php-src falls through into state 9 for the final byte of the line.
            return self.emit_word(index, true);
        }
        if self.strict() && !matches!(byte, b'\r' | b'\n' | b' ' | b'\t') {
            let start = self.encoded_word.unwrap_or(index);
            self.append_literal_range(start, index + 1)?;
            self.state = State::LiteralWord;
            return Ok(());
        }
        self.emit_word(index, false)
    }

    /// Decodes and appends the completed encoded-word, then selects the next state.
    fn emit_word(&mut self, index: usize, eos: bool) -> IconvResult<()> {
        let (start, end) = self.payload.take().unwrap_or((index, index));
        let raw = &self.input[start..end];
        let decoded = if self.base64_payload {
            base64::decode(raw)
        } else {
            quoted_printable::decode(raw)
        };
        let converted = match self.word_converter.as_mut() {
            Some(converter) => converter.convert_all(&decoded),
            None => Err(IconvError::MalformedString),
        };
        match converted {
            Ok(bytes) => {
                self.spaces = None;
                self.out.extend_from_slice(&bytes);
            }
            Err(error) => {
                if !self.lenient() {
                    return Err(self.charset_error(error));
                }
                // php-src re-emits the word without the byte that follows it.
                let word_start = self.encoded_word.unwrap_or(index);
                self.append_literal_range(word_start, index)?;
                self.encoded_word = None;
            }
        }
        if eos {
            self.state = State::Any;
            return Ok(());
        }
        match self.input[index] {
            b'\r' => self.state = State::LineFeed,
            b'\n' => self.state = State::FoldCheck,
            b'=' => self.state = State::OpenDelimiter,
            b' ' | b'\t' => {
                self.spaces = Some(index);
                self.state = State::Whitespace;
            }
            byte => {
                let _ = self.append_plain(&[byte]);
                self.state = State::LiteralWord;
            }
        }
        Ok(())
    }

    /// State 7: a carriage return that is not followed by a newline is literal text.
    fn scan_line_feed(&mut self, byte: u8) {
        if byte == b'\n' {
            self.state = State::FoldCheck;
            return;
        }
        let _ = self.append_plain(&[b'\r', byte]);
        self.state = State::Any;
    }

    /// State 8: linear whitespace folds the next line in; anything else ends the header.
    ///
    /// Returns whether the header ended.
    fn scan_fold(&mut self, byte: u8) -> bool {
        if byte != b' ' && byte != b'\t' {
            return true;
        }
        // A fold directly after an encoded-word contributes nothing.
        if self.encoded_word.is_none() {
            let _ = self.append_plain(b" ");
        }
        self.spaces = None;
        self.state = State::Whitespace;
        false
    }

    /// State 11: whitespace is held back until its successor decides its fate.
    fn scan_whitespace(&mut self, byte: u8, index: usize) -> IconvResult<()> {
        match byte {
            b'\r' => self.state = State::LineFeed,
            b'\n' => self.state = State::FoldCheck,
            b'=' => {
                if self.spaces.is_some() && self.encoded_word.is_none() {
                    let start = self.spaces.take().unwrap_or(index);
                    // php-src ignores this conversion's outcome.
                    let _ = self.append_literal_range(start, index);
                }
                self.encoded_word = Some(index);
                self.state = State::OpenDelimiter;
            }
            b' ' | b'\t' => {}
            byte => {
                if let Some(start) = self.spaces.take() {
                    // php-src ignores this conversion's outcome.
                    let _ = self.append_literal_range(start, index);
                }
                let _ = self.append_plain(&[byte]);
                self.encoded_word = None;
                self.state = self.literal_state();
            }
        }
        Ok(())
    }

    /// State 12: literal text that only opens an encoded-word outside strict mode.
    fn scan_literal_word(&mut self, byte: u8, index: usize) {
        match byte {
            b'\r' => self.state = State::LineFeed,
            b'\n' => self.state = State::FoldCheck,
            b' ' | b'\t' => {
                self.spaces = Some(index);
                self.state = State::Whitespace;
            }
            b'=' if !self.strict() => {
                self.encoded_word = Some(index);
                self.state = State::OpenDelimiter;
            }
            byte => {
                let _ = self.append_plain(&[byte]);
            }
        }
    }

    /// Applies php-src's end-of-input rule for a half-scanned encoded-word.
    fn finish_line(&mut self) -> IconvResult<()> {
        match self.state {
            State::Any | State::FoldCheck | State::Whitespace | State::LiteralWord => Ok(()),
            _ => {
                if !self.lenient() {
                    return Err(IconvError::MalformedString);
                }
                if self.state == State::OpenDelimiter {
                    let _ = self.append_plain(b"=");
                }
                Ok(())
            }
        }
    }

    /// Re-emits a failed encoded-word candidate as literal text, or reports it malformed.
    fn reject_word(&mut self, end: usize) -> IconvResult<()> {
        if !self.lenient() {
            return Err(IconvError::MalformedString);
        }
        let start = self.encoded_word.unwrap_or(end.saturating_sub(1));
        self.append_literal_range(start, end)?;
        self.encoded_word = None;
        self.state = self.literal_state();
        Ok(())
    }

    /// Converts one literal input range from ASCII into the target charset.
    fn append_literal_range(&mut self, start: usize, end: usize) -> IconvResult<()> {
        if start >= end {
            return Ok(());
        }
        let literal = self.input[start..end].to_vec();
        self.append_plain(&literal)
    }

    /// Converts literal header text from ASCII, one byte at a time like php-src.
    fn append_plain(&mut self, bytes: &[u8]) -> IconvResult<()> {
        for byte in bytes {
            let converted = self.plain.convert_stream(&[*byte])?;
            self.out.extend_from_slice(&converted);
        }
        Ok(())
    }

    /// Rewrites a charset failure with the placeholder source name php-src reports.
    fn charset_error(&self, error: IconvError) -> IconvError {
        error.with_reported_charsets(REPORTED_SOURCE_CHARSET, &self.reported)
    }
}
