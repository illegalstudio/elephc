//! Purpose:
//! Binds the libc `iconv` conversion API and wraps a descriptor in a safe RAII handle
//! that reports php-src's failure classification.
//!
//! Called from:
//! - `crate::convert`, `crate::text`, and `crate::mime` whenever bytes must be transcoded.
//!
//! Key details:
//! - macOS resolves `iconv_open` from `libiconv`, so the extern block links it there;
//!   glibc and musl provide the same symbols from libc itself.
//! - Opening a converter first installs a UTF-8 `LC_CTYPE`, because glibc drives
//!   `//TRANSLIT` from that locale and the PHP CLI installs one at startup too.
//! - `Converter::convert_all` grows its own output buffer, so callers never handle `E2BIG`.
//! - A charset name containing an interior NUL can never reach libc, and is reported as
//!   an unusable charset exactly like an unknown name.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Once;

use crate::error::{IconvError, IconvResult};

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
    fn iconv_close(cd: *mut c_void) -> c_int;
    fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
}

/// POSIX `LC_CTYPE` category number, identical on Linux and macOS' BSD libc.
#[cfg(target_os = "macos")]
const LC_CTYPE: c_int = 0;
#[cfg(not(target_os = "macos"))]
const LC_CTYPE: c_int = 0;

/// Guards the one-time character-classification locale setup.
static LOCALE_INIT: Once = Once::new();

/// Selects a UTF-8 character-classification locale once per process.
///
/// glibc resolves `//TRANSLIT` through `LC_CTYPE`, so a program left in the default `C`
/// locale transliterates every non-ASCII character to `?`. The PHP CLI installs a UTF-8
/// `LC_CTYPE` at startup, and matching that is what makes `iconv($from, "ASCII//TRANSLIT",
/// $text)` produce the same bytes here as it does under php. Only `LC_CTYPE` is touched,
/// and only the first time a conversion is opened.
fn ensure_ctype_locale() {
    LOCALE_INIT.call_once(|| {
        for name in [c"C.UTF-8", c"", c"UTF-8"] {
            let applied = unsafe { setlocale(LC_CTYPE, name.as_ptr()) };
            if !applied.is_null() {
                return;
            }
        }
    });
}

/// POSIX `E2BIG`: the output buffer ran out of room.
const E2BIG: i32 = 7;
/// POSIX `EINVAL`: the input ends inside a valid but truncated sequence.
const EINVAL: i32 = 22;
/// POSIX `EILSEQ`: the input contains a sequence the source charset forbids.
///
/// The value is target-specific: Linux uses 84 while the BSD-derived macOS libc uses 92.
#[cfg(target_os = "macos")]
const EILSEQ: i32 = 92;
#[cfg(not(target_os = "macos"))]
const EILSEQ: i32 = 84;

/// Number of output bytes requested from libc per conversion round.
const CHUNK: usize = 256;

/// One open libc conversion descriptor, closed when the wrapper is dropped.
pub struct Converter {
    descriptor: *mut c_void,
}

impl Converter {
    /// Opens a descriptor converting `from` into `to`, or reports php-src's charset error.
    ///
    /// The error carries the charset names exactly as the caller spelled them so the
    /// rendered diagnostic matches php-src's `Wrong encoding, conversion from ...` text.
    pub fn open(from: &[u8], to: &[u8]) -> IconvResult<Self> {
        ensure_ctype_locale();
        let wrong_charset = || IconvError::WrongCharset {
            from: String::from_utf8_lossy(from).into_owned(),
            to: String::from_utf8_lossy(to).into_owned(),
        };
        let from_c = CString::new(from).map_err(|_| wrong_charset())?;
        let to_c = CString::new(to).map_err(|_| wrong_charset())?;
        let descriptor = unsafe { iconv_open(to_c.as_ptr(), from_c.as_ptr()) };
        if descriptor as isize == -1 {
            return Err(wrong_charset());
        }
        Ok(Self { descriptor })
    }

    /// Converts `input` completely, returning the transcoded bytes.
    ///
    /// Grows the destination until libc stops reporting `E2BIG`, then flushes any
    /// closing shift sequence. A truncated tail becomes `IncompleteChar` and a rejected
    /// sequence becomes `IllegalSequence`, matching php-src's `php_iconv_string`.
    pub fn convert_all(&mut self, input: &[u8]) -> IconvResult<Vec<u8>> {
        let mut output = Vec::with_capacity(input.len() + CHUNK);
        let mut cursor = input.as_ptr().cast_mut().cast::<c_char>();
        let mut left = input.len();
        while left > 0 {
            let produced = self.round(&mut cursor, &mut left, &mut output)?;
            if produced == 0 && left > 0 {
                // No forward progress with a fresh buffer means the descriptor is stuck.
                return Err(IconvError::IllegalSequence);
            }
        }
        self.flush(&mut output)?;
        Ok(output)
    }

    /// Converts `input` completely, optionally skipping bytes the source charset rejects.
    ///
    /// glibc reports `EILSEQ` even for a `//IGNORE` target, so php-src implements the
    /// skipping itself; `ignore_illegal` reproduces that loop. php-src also stops without
    /// an error once a single rejected byte is all that is left, dropping it.
    pub fn convert_all_ignoring(&mut self, input: &[u8], ignore_illegal: bool) -> IconvResult<Vec<u8>> {
        if !ignore_illegal {
            return self.convert_all(input);
        }
        let mut output = Vec::with_capacity(input.len() + CHUNK);
        let mut cursor = input;
        while !cursor.is_empty() {
            let start = output.len();
            output.resize(start + CHUNK, 0);
            let before = cursor.len();
            let (produced, failure) = {
                let (produced, failure) = self.step(&mut cursor, &mut output[start..], true);
                (produced, failure)
            };
            output.truncate(start + produced);
            match failure {
                Some(IconvError::IllegalSequence) => {
                    if cursor.len() <= 1 {
                        break;
                    }
                    cursor = &cursor[1..];
                }
                Some(error) => return Err(error),
                None => {
                    if produced == 0 && cursor.len() == before {
                        return Err(IconvError::IllegalSequence);
                    }
                }
            }
        }
        let mut tail = Vec::new();
        self.flush(&mut tail)?;
        output.extend_from_slice(&tail);
        Ok(output)
    }

    /// Converts `input` without emitting the descriptor's closing shift sequence.
    ///
    /// MIME decoding appends many small literal runs to one output stream, and php-src
    /// never emits a closing shift sequence for that converter, so neither does this.
    pub fn convert_stream(&mut self, input: &[u8]) -> IconvResult<Vec<u8>> {
        let mut output = Vec::with_capacity(input.len() + 8);
        let mut cursor = input.as_ptr().cast_mut().cast::<c_char>();
        let mut left = input.len();
        while left > 0 {
            let produced = self.round(&mut cursor, &mut left, &mut output)?;
            if produced == 0 && left > 0 {
                return Err(IconvError::IllegalSequence);
            }
        }
        Ok(output)
    }

    /// Runs one bounded conversion round and appends whatever libc produced.
    fn round(
        &mut self,
        cursor: &mut *mut c_char,
        left: &mut usize,
        output: &mut Vec<u8>,
    ) -> IconvResult<usize> {
        let start = output.len();
        output.resize(start + CHUNK, 0);
        let mut out_ptr = unsafe { output.as_mut_ptr().add(start) }.cast::<c_char>();
        let mut out_left = CHUNK;
        let status = unsafe { iconv(self.descriptor, cursor, left, &mut out_ptr, &mut out_left) };
        let produced = CHUNK - out_left;
        output.truncate(start + produced);
        if status != usize::MAX {
            return Ok(produced);
        }
        match last_errno() {
            E2BIG => Ok(produced),
            EINVAL => Err(IconvError::IncompleteChar),
            EILSEQ => Err(IconvError::IllegalSequence),
            _ => Err(IconvError::IllegalSequence),
        }
    }

    /// Emits the descriptor's closing shift sequence into `output`.
    fn flush(&mut self, output: &mut Vec<u8>) -> IconvResult<()> {
        let start = output.len();
        output.resize(start + CHUNK, 0);
        let mut out_ptr = unsafe { output.as_mut_ptr().add(start) }.cast::<c_char>();
        let mut out_left = CHUNK;
        let status = unsafe {
            iconv(
                self.descriptor,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut out_ptr,
                &mut out_left,
            )
        };
        output.truncate(start + (CHUNK - out_left));
        if status == usize::MAX {
            return Err(IconvError::IllegalSequence);
        }
        Ok(())
    }

    /// Converts into a fixed buffer, reporting how many bytes were produced.
    ///
    /// `E2BIG` is not an error here: it simply means the buffer filled up, which is how
    /// `iconv_mime_encode()` discovers how much input fits in one encoded-word. The
    /// second element of the result reports whether that happened.
    pub fn convert_into(
        &mut self,
        input: &mut &[u8],
        out: &mut [u8],
    ) -> IconvResult<(usize, bool)> {
        let mut cursor = input.as_ptr().cast_mut().cast::<c_char>();
        let mut left = input.len();
        let mut out_ptr = out.as_mut_ptr().cast::<c_char>();
        let mut out_left = out.len();
        let status =
            unsafe { iconv(self.descriptor, &mut cursor, &mut left, &mut out_ptr, &mut out_left) };
        let produced = out.len() - out_left;
        *input = &input[input.len() - left..];
        if status != usize::MAX {
            return Ok((produced, false));
        }
        match last_errno() {
            E2BIG => Ok((produced, true)),
            EINVAL => Err(IconvError::IncompleteChar),
            EILSEQ => Err(IconvError::IllegalSequence),
            _ => Err(IconvError::IllegalSequence),
        }
    }

    /// Emits the closing shift sequence into a fixed buffer.
    ///
    /// Reports whether the buffer was too small, which `iconv_mime_encode()` answers by
    /// reserving more room and converting less input.
    pub fn flush_into(&mut self, out: &mut [u8]) -> IconvResult<(usize, bool)> {
        let mut out_ptr = out.as_mut_ptr().cast::<c_char>();
        let mut out_left = out.len();
        let status = unsafe {
            iconv(
                self.descriptor,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut out_ptr,
                &mut out_left,
            )
        };
        let produced = out.len() - out_left;
        if status != usize::MAX {
            return Ok((produced, false));
        }
        match last_errno() {
            E2BIG => Ok((produced, true)),
            _ => Err(IconvError::IllegalSequence),
        }
    }

    /// Runs one conversion step, reporting production and failure separately.
    ///
    /// php-src's character-oriented scanners need both facts at once: they stop as soon
    /// as a step produces nothing, and they remember a failure without abandoning the
    /// characters produced so far. `feed` selects between converting more input and
    /// emitting the closing shift sequence.
    pub fn step(
        &mut self,
        input: &mut &[u8],
        out: &mut [u8],
        feed: bool,
    ) -> (usize, Option<IconvError>) {
        let mut cursor = input.as_ptr().cast_mut().cast::<c_char>();
        let mut left = input.len();
        let mut out_ptr = out.as_mut_ptr().cast::<c_char>();
        let mut out_left = out.len();
        let status = unsafe {
            if feed {
                iconv(
                    self.descriptor,
                    &mut cursor,
                    &mut left,
                    &mut out_ptr,
                    &mut out_left,
                )
            } else {
                iconv(
                    self.descriptor,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut out_ptr,
                    &mut out_left,
                )
            }
        };
        let produced = out.len() - out_left;
        if feed {
            *input = &input[input.len() - left..];
        }
        if status != usize::MAX {
            return (produced, None);
        }
        let failure = match last_errno() {
            E2BIG => None,
            EINVAL => Some(IconvError::IncompleteChar),
            EILSEQ => Some(IconvError::IllegalSequence),
            _ => Some(IconvError::IllegalSequence),
        };
        (produced, failure)
    }

    /// Resets the descriptor's shift state without emitting anything.
    pub fn reset(&mut self) {
        let mut sink = [0u8; 8];
        let mut out_ptr = sink.as_mut_ptr().cast::<c_char>();
        let mut out_left = sink.len();
        unsafe {
            iconv(
                self.descriptor,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut out_ptr,
                &mut out_left,
            );
        }
    }
}

impl Drop for Converter {
    /// Closes the libc descriptor owned by this wrapper.
    fn drop(&mut self) {
        unsafe {
            iconv_close(self.descriptor);
        }
    }
}

/// Returns whether libc can convert between the two charsets at all.
pub fn charsets_are_convertible(from: &[u8], to: &[u8]) -> bool {
    Converter::open(from, to).is_ok()
}

/// Reads the platform `errno` value libc's `iconv` just published.
fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
