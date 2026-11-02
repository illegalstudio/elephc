//! Purpose:
//! Opaque optional-provider regex engine used by eval `preg_*` builtins.
//! Provides the small capture API the eval regex modules need without making
//! libelephc_magician itself depend on PCRE2.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::pattern`.
//! - `crate::interpreter::builtins::regex` match, replace, and split helpers.
//!
//! Key details:
//! - Subject and pattern bytes are passed through the registered provider as C strings.
//! - Match offsets are byte offsets into the original subject, matching PHP capture arrays.
//! - A global iterator keeps its C-string allocation alive across native continuation calls.

use std::ffi::{c_char, c_int, c_void, CString};
use std::marker::PhantomData;

use super::super::super::EvalStatus;
use crate::regex_provider::{regex_provider, RegexProvider};

const REG_ICASE: c_int = 0x0001;
const REG_NEWLINE: c_int = 0x0002;
const REG_DOTALL: c_int = 0x0010;
const REG_STARTEND: c_int = 0x0080;
const ELEPHC_PCRE2_GLOBAL_NEXT: u32 = 1 << 16;
const REG_UNGREEDY: c_int = 0x0200;
const REG_UCP: c_int = 0x0400;
const REG_UTF: c_int = 0x0040;
const ELEPHC_PCRE2_CFLAG_ANCHORED: c_int = 0x2000;
// Elephc-owned bits above POSIX's own range: pcre2posix.h has no REG_* bit
// for PCRE2_EXTENDED, PCRE2_DOLLAR_ENDONLY, PCRE2_DUPNAMES, or
// PCRE2_NO_AUTO_CAPTURE, so `elephc_pcre2_v1_compile()` translates these
// directly into native PCRE2 compile options instead of POSIX cflags. Must
// stay numerically identical to the shim in
// `src/native_deps/recipes/pcre2_shim.c` and to the `#[cfg(test)]` provider
// in `crate::regex_provider`.
const ELEPHC_PCRE2_CFLAG_EXTENDED: c_int = 0x4000;
const ELEPHC_PCRE2_CFLAG_DOLLAR_ENDONLY: c_int = 0x8000;
const ELEPHC_PCRE2_CFLAG_DUPNAMES: c_int = 0x10000;
const ELEPHC_PCRE2_CFLAG_NO_AUTO_CAPTURE: c_int = 0x20000;
const REG_NOMATCH: c_int = 17;

/// Copies a subject and adds the trailing terminator the provider's C-string argument needs.
///
/// REG_STARTEND bounds the search by the offsets, not by a terminator, so a subject holding NUL
/// bytes is searched whole. A `CString` refused such a subject, and the call answered "no match".
fn nul_terminated(subject: &[u8]) -> Vec<u8> {
    let mut subject_c = Vec::with_capacity(subject.len() + 1);
    subject_c.extend_from_slice(subject);
    subject_c.push(0);
    subject_c
}

/// Spells every NUL byte of a pattern body as the PCRE escape `\x00`.
///
/// php hands PCRE2 the pattern with its length, so a raw NUL is just a character to match --
/// Laravel's `Str::trim()` puts `"\0"` inside a character class. The provider takes a C string,
/// which cannot hold one: the build of that string failed and the whole call became an
/// interpreter error. The escape means the same thing to PCRE2 in and out of a class. A NUL that
/// an odd run of backslashes already escapes gets only `x00`, so the backslash in front of it
/// becomes the escape's own.
pub(in crate::interpreter) fn pattern_with_escaped_nul_bytes(body: &[u8]) -> Vec<u8> {
    if !body.contains(&0) {
        return body.to_vec();
    }
    let mut escaped = Vec::with_capacity(body.len() + 8);
    let mut pending_backslash = false;
    for &byte in body {
        if byte == 0 {
            if pending_backslash {
                escaped.extend_from_slice(b"x00");
            } else {
                escaped.extend_from_slice(b"\\x00");
            }
            pending_backslash = false;
            continue;
        }
        escaped.push(byte);
        pending_backslash = byte == b'\\' && !pending_backslash;
    }
    escaped
}

/// Supported PHP regex modifiers after delimiter stripping.
#[derive(Default)]
pub(in crate::interpreter) struct EvalPregModifiers {
    pub(in crate::interpreter) case_insensitive: bool,
    pub(in crate::interpreter) multi_line: bool,
    pub(in crate::interpreter) dot_matches_new_line: bool,
    pub(in crate::interpreter) swap_greed: bool,
    pub(in crate::interpreter) unicode: bool,
    pub(in crate::interpreter) anchored: bool,
    /// PHP's `x` modifier (PCRE2_EXTENDED): ignores unescaped pattern
    /// whitespace and `#...` comments outside character classes.
    pub(in crate::interpreter) extended: bool,
    /// PHP's `D` modifier (PCRE2_DOLLAR_ENDONLY): `$` matches only at the
    /// exact end of the subject, not before a trailing newline.
    pub(in crate::interpreter) dollar_endonly: bool,
    /// PHP's `J` modifier (PCRE2_DUPNAMES): allows more than one capture
    /// group to declare the same name.
    pub(in crate::interpreter) dupnames: bool,
    /// PHP's `n` modifier (PCRE2_NO_AUTO_CAPTURE): unnamed `(...)` groups
    /// stop capturing and stop consuming a capture-group number; named
    /// groups are unaffected and keep capturing.
    pub(in crate::interpreter) no_auto_capture: bool,
}

/// A compiled regex plus its registered opaque provider.
pub(in crate::interpreter) struct Regex {
    handle: *mut c_void,
    capture_slots: usize,
    provider: RegexProvider,
}

impl Regex {
    /// Compiles a delimiter-stripped pattern with PHP regex modifiers.
    pub(in crate::interpreter) fn compile(
        body: &[u8],
        modifiers: EvalPregModifiers,
    ) -> Result<Self, EvalStatus> {
        let provider = regex_provider().ok_or(EvalStatus::RuntimeFatal)?;
        let pattern =
            CString::new(pattern_with_escaped_nul_bytes(body)).map_err(|_| EvalStatus::RuntimeFatal)?;
        let mut handle = std::ptr::null_mut();
        let mut capture_slots = 0_u64;
        let status = unsafe {
            (provider.compile)(
                &mut handle,
                pattern.as_ptr(),
                modifiers.flags() as u32,
                &mut capture_slots,
            )
        };
        let Ok(capture_slots) = usize::try_from(capture_slots) else {
            if !handle.is_null() {
                unsafe { (provider.free)(handle) };
            }
            return Err(EvalStatus::RuntimeFatal);
        };
        if status != 0 || handle.is_null() || capture_slots == 0 {
            if !handle.is_null() {
                unsafe { (provider.free)(handle) };
            }
            return Err(EvalStatus::RuntimeFatal);
        }
        Ok(Self {
            handle,
            capture_slots,
            provider,
        })
    }

    /// Compiles a pattern for the PHP `function`, or reports why php rejects it.
    ///
    /// A pattern PCRE2 cannot compile -- or one the delimiter parser already rejected, which
    /// makes this compile fail on purpose -- produces php's warning and `None`; the caller then
    /// returns its false or null. Any other failure stays an interpreter error.
    pub(in crate::interpreter) fn compile_or_warn(
        body: &[u8],
        modifiers: EvalPregModifiers,
        function: &str,
        values: &mut impl crate::interpreter::RuntimeValueOps,
    ) -> Result<Option<Self>, EvalStatus> {
        match Self::compile(body, modifiers) {
            Ok(regex) => Ok(Some(regex)),
            Err(status) => match crate::regex_provider::regex_compile_warning(function) {
                Some(message) => {
                    values.warning(&message)?;
                    Ok(None)
                }
                None => Err(status),
            },
        }
    }

    /// Returns the number of capture slots including the full match at index 0.
    pub(in crate::interpreter) fn captures_len(&self) -> usize {
        self.capture_slots
    }

    /// Returns how many capture groups this compiled pattern named.
    ///
    /// Zero means `$matches` stays PHP's plain indexed array; PCRE2 owns the
    /// name table, so this never re-derives names by parsing the pattern.
    pub(in crate::interpreter) fn name_count(&self) -> usize {
        let count = unsafe { (self.provider.name_count)(self.handle) };
        usize::try_from(count).unwrap_or(0)
    }

    /// Returns the declared name for one capture-group index, if PCRE2's
    /// name table has one, as raw bytes (PCRE2 group names are ASCII, but
    /// bytes avoid an unnecessary UTF-8 validation panic).
    /// Returns the name set by the MARK verb this regex's most recent match passed.
    ///
    /// PHP exposes it as `$matches['MARK']`, and Symfony's dumped `CompiledUrlMatcher` indexes
    /// `$this->dynamicRoutes` by it, so without it every route with a placeholder resolves to
    /// entry 0 and fails.
    pub(in crate::interpreter) fn last_mark(&self) -> Option<Vec<u8>> {
        let mut mark_ptr: *const std::ffi::c_char = std::ptr::null();
        let mut mark_len: u64 = 0;
        let status = unsafe { (self.provider.last_mark)(self.handle, &mut mark_ptr, &mut mark_len) };
        if status != 0 || mark_ptr.is_null() {
            return None;
        }
        let len = usize::try_from(mark_len).ok()?;
        Some(unsafe { std::slice::from_raw_parts(mark_ptr.cast::<u8>(), len) }.to_vec())
    }

    pub(in crate::interpreter) fn group_name(&self, group: usize) -> Option<Vec<u8>> {
        let group = u64::try_from(group).ok()?;
        let mut name_ptr: *const c_char = std::ptr::null();
        let mut name_len: u64 = 0;
        let status =
            unsafe { (self.provider.group_name)(self.handle, group, &mut name_ptr, &mut name_len) };
        if status != 0 || name_ptr.is_null() {
            return None;
        }
        let len = usize::try_from(name_len).ok()?;
        let bytes = unsafe { std::slice::from_raw_parts(name_ptr.cast::<u8>(), len) };
        Some(bytes.to_vec())
    }

    /// Returns the first capture set for this regex and subject.
    pub(in crate::interpreter) fn captures<'a>(&self, subject: &'a [u8]) -> Option<Captures<'a>> {
        self.captures_at(subject, 0)
    }

    /// Returns the first capture set at or after a byte offset in the subject.
    pub(in crate::interpreter) fn captures_at<'a>(
        &self,
        subject: &'a [u8],
        start: usize,
    ) -> Option<Captures<'a>> {
        self.exec_at(subject, start)
    }

    /// Returns every non-overlapping capture set for this regex and subject.
    pub(in crate::interpreter) fn captures_iter<'a>(
        &self,
        subject: &'a [u8],
    ) -> std::vec::IntoIter<Captures<'a>> {
        self.captures_iter_at(subject, 0)
    }

    /// Returns every non-overlapping capture set at or after a byte offset.
    pub(in crate::interpreter) fn captures_iter_at<'a>(
        &self,
        subject: &'a [u8],
        start: usize,
    ) -> std::vec::IntoIter<Captures<'a>> {
        let mut captures = Vec::new();
        // The first search starts at `start`; later ones continue from the previous match through
        // the provider's global-next mode, which keeps anchors, word boundaries and lookbehind
        // anchored to the whole subject and steps past an empty match the way PHP does.
        let subject_c = nul_terminated(subject);
        let mut continuation = false;
        loop {
            let Some(next) = self.exec_subject(subject, &subject_c, start, continuation) else {
                break;
            };
            if next.get(0).is_none() {
                break;
            }
            captures.push(next);
            continuation = true;
        }
        captures.into_iter()
    }

    /// Executes this regex from a byte offset, returning capture offsets on match.
    fn exec_at<'a>(&self, subject: &'a [u8], start: usize) -> Option<Captures<'a>> {
        let subject_c = nul_terminated(subject);
        self.exec_subject(subject, &subject_c, start, false)
    }

    /// Executes with stable C-string storage while preserving the original Rust subject lifetime.
    fn exec_subject<'a>(
        &self, subject: &'a [u8], subject_c: &[u8], start: usize, continuation: bool,
    ) -> Option<Captures<'a>> {
        let mut offset_pairs = vec![-1_i64; self.captures_len().checked_mul(2)?];
        offset_pairs[0] = i64::try_from(start).ok()?;
        offset_pairs[1] = i64::try_from(subject.len()).ok()?;
        let status = unsafe {
            (self.provider.exec)(
                self.handle,
                subject_c.as_ptr().cast::<c_char>(),
                u64::try_from(self.captures_len()).ok()?,
                offset_pairs.as_mut_ptr(),
                REG_STARTEND as u32 | if continuation { ELEPHC_PCRE2_GLOBAL_NEXT } else { 0 },
            )
        };
        if status == REG_NOMATCH || status != 0 {
            return None;
        }
        Some(Captures {
            matches: offset_pairs
                .chunks_exact(2)
                .map(|pair| offsets(pair[0], pair[1]))
                .collect(),
            _subject: PhantomData,
        })
    }
}

impl Drop for Regex {
    /// Releases the compiled regex through the provider that created it.
    fn drop(&mut self) {
        unsafe { (self.provider.free)(self.handle) };
    }
}

impl EvalPregModifiers {
    /// Converts parsed PHP modifiers into PCRE2 POSIX compile flags.
    fn flags(&self) -> c_int {
        let mut flags = 0;
        if self.case_insensitive {
            flags |= REG_ICASE;
        }
        if self.multi_line {
            flags |= REG_NEWLINE;
        }
        if self.dot_matches_new_line {
            flags |= REG_DOTALL;
        }
        if self.swap_greed {
            flags |= REG_UNGREEDY;
        }
        if self.unicode {
            flags |= REG_UTF | REG_UCP;
        }
        if self.anchored {
            flags |= ELEPHC_PCRE2_CFLAG_ANCHORED;
        }
        if self.extended {
            flags |= ELEPHC_PCRE2_CFLAG_EXTENDED;
        }
        if self.dollar_endonly {
            flags |= ELEPHC_PCRE2_CFLAG_DOLLAR_ENDONLY;
        }
        if self.dupnames {
            flags |= ELEPHC_PCRE2_CFLAG_DUPNAMES;
        }
        if self.no_auto_capture {
            flags |= ELEPHC_PCRE2_CFLAG_NO_AUTO_CAPTURE;
        }
        flags
    }
}

/// Converts one provider offset pair to an optional Rust byte range.
fn offsets(start: i64, end: i64) -> Option<(usize, usize)> {
    Some((usize::try_from(start).ok()?, usize::try_from(end).ok()?))
}

/// One regex match span.
#[derive(Clone, Copy)]
pub(in crate::interpreter) struct Match {
    start: usize,
    end: usize,
}

impl Match {
    /// Returns the match start byte offset.
    pub(in crate::interpreter) fn start(&self) -> usize {
        self.start
    }

    /// Returns the match end byte offset.
    pub(in crate::interpreter) fn end(&self) -> usize {
        self.end
    }
}

/// Capture offsets for one regex match.
pub(in crate::interpreter) struct Captures<'a> {
    matches: Vec<Option<(usize, usize)>>,
    _subject: PhantomData<&'a [u8]>,
}

impl Captures<'_> {
    /// Returns the number of capture slots including the full match.
    pub(in crate::interpreter) fn len(&self) -> usize {
        self.matches.len()
    }

    /// Returns the match span for one capture slot.
    pub(in crate::interpreter) fn get(&self, index: usize) -> Option<Match> {
        let (start, end) = self.matches.get(index).copied().flatten()?;
        Some(Match { start, end })
    }
}
