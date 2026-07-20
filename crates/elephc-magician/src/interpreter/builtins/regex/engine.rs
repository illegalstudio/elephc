//! Purpose:
//! PCRE2 POSIX-wrapper regex engine used by eval `preg_*` builtins.
//! Provides the small capture API the eval regex modules need while sharing
//! the same native regex family as the AOT runtime path.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::pattern`.
//! - `crate::interpreter::builtins::regex` match, replace, and split helpers.
//!
//! Key details:
//! - Subject and pattern bytes are passed through PCRE2's POSIX wrapper as C strings.
//! - Match offsets are byte offsets into the original subject, matching PHP capture arrays.

use std::ffi::CString;
use std::marker::PhantomData;

use libc::{c_char, c_int, c_void, size_t};

use super::super::super::EvalStatus;

const REG_ICASE: c_int = 0x0001;
const REG_NEWLINE: c_int = 0x0002;
const REG_DOTALL: c_int = 0x0010;
const REG_STARTEND: c_int = 0x0080;
const REG_UNGREEDY: c_int = 0x0200;
const REG_UCP: c_int = 0x0400;
const REG_UTF: c_int = 0x0040;
const REG_NOMATCH: c_int = 17;

/// PCRE2 POSIX `regex_t` layout for the supported PCRE2 wrapper ABI.
#[repr(C)]
struct Pcre2Regex {
    re_pcre2_code: *mut c_void,
    re_match_data: *mut c_void,
    re_endp: *const c_char,
    re_nsub: size_t,
    re_erroffset: size_t,
    re_cflags: c_int,
}

/// PCRE2 POSIX `regmatch_t` capture offset pair.
#[repr(C)]
#[derive(Clone, Copy)]
struct Pcre2Regmatch {
    rm_so: c_int,
    rm_eo: c_int,
}

unsafe extern "C" {
    /// Compiles a PCRE2 pattern through the POSIX wrapper.
    fn pcre2_regcomp(regex: *mut Pcre2Regex, pattern: *const c_char, flags: c_int) -> c_int;

    /// Executes a compiled PCRE2 regex and fills capture offsets.
    fn pcre2_regexec(
        regex: *const Pcre2Regex,
        subject: *const c_char,
        nmatch: size_t,
        matches: *mut Pcre2Regmatch,
        flags: c_int,
    ) -> c_int;

    /// Releases resources owned by a compiled PCRE2 regex.
    fn pcre2_regfree(regex: *mut Pcre2Regex);
}

/// Supported PHP regex modifiers after delimiter stripping.
#[derive(Default)]
pub(in crate::interpreter) struct EvalPregModifiers {
    pub(in crate::interpreter) case_insensitive: bool,
    pub(in crate::interpreter) multi_line: bool,
    pub(in crate::interpreter) dot_matches_new_line: bool,
    pub(in crate::interpreter) swap_greed: bool,
    pub(in crate::interpreter) unicode: bool,
}

/// A compiled PCRE2 regex plus POSIX wrapper metadata.
pub(in crate::interpreter) struct Regex {
    raw: Pcre2Regex,
}

impl Regex {
    /// Compiles a delimiter-stripped pattern with PHP regex modifiers.
    pub(in crate::interpreter) fn compile(
        body: &[u8],
        modifiers: EvalPregModifiers,
    ) -> Result<Self, EvalStatus> {
        let pattern = CString::new(body).map_err(|_| EvalStatus::RuntimeFatal)?;
        let mut raw = Pcre2Regex {
            re_pcre2_code: std::ptr::null_mut(),
            re_match_data: std::ptr::null_mut(),
            re_endp: std::ptr::null(),
            re_nsub: 0,
            re_erroffset: 0,
            re_cflags: 0,
        };
        let status = unsafe { pcre2_regcomp(&mut raw, pattern.as_ptr(), modifiers.flags()) };
        if status != 0 {
            return Err(EvalStatus::RuntimeFatal);
        }
        Ok(Self { raw })
    }

    /// Returns the number of capture slots including the full match at index 0.
    pub(in crate::interpreter) fn captures_len(&self) -> usize {
        self.raw.re_nsub.saturating_add(1)
    }

    /// Returns whether this regex matches the subject.
    pub(in crate::interpreter) fn is_match(&self, subject: &[u8]) -> bool {
        self.captures(subject).is_some()
    }

    /// Returns the first capture set for this regex and subject.
    pub(in crate::interpreter) fn captures<'a>(&self, subject: &'a [u8]) -> Option<Captures<'a>> {
        self.exec_at(subject, 0)
    }

    /// Returns every non-overlapping capture set for this regex and subject.
    pub(in crate::interpreter) fn captures_iter<'a>(
        &self,
        subject: &'a [u8],
    ) -> std::vec::IntoIter<Captures<'a>> {
        let mut captures = Vec::new();
        let mut cursor = 0;
        while cursor <= subject.len() {
            let Some(next) = self.exec_at(subject, cursor) else {
                break;
            };
            let Some(full_match) = next.get(0) else {
                break;
            };
            let end = full_match.end();
            let start = full_match.start();
            captures.push(next);
            if end > cursor {
                cursor = end;
            } else if start < subject.len() {
                cursor = start + 1;
            } else {
                break;
            }
        }
        captures.into_iter()
    }

    /// Executes this regex from a byte offset, returning capture offsets on match.
    fn exec_at<'a>(&self, subject: &'a [u8], start: usize) -> Option<Captures<'a>> {
        let subject_c = CString::new(subject).ok()?;
        let mut matches = vec![Pcre2Regmatch::unmatched(); self.captures_len().max(1)];
        matches[0].rm_so = c_int::try_from(start).ok()?;
        matches[0].rm_eo = c_int::try_from(subject.len()).ok()?;
        let status = unsafe {
            pcre2_regexec(
                &self.raw,
                subject_c.as_ptr(),
                matches.len(),
                matches.as_mut_ptr(),
                REG_STARTEND,
            )
        };
        if status == REG_NOMATCH || status != 0 {
            return None;
        }
        Some(Captures {
            matches: matches
                .into_iter()
                .map(|matched| matched.to_offsets())
                .collect(),
            _subject: PhantomData,
        })
    }
}

impl Drop for Regex {
    /// Releases the compiled PCRE2 regex when the wrapper is dropped.
    fn drop(&mut self) {
        unsafe { pcre2_regfree(&mut self.raw) };
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
        flags
    }
}

impl Pcre2Regmatch {
    /// Returns an unmatched capture sentinel.
    fn unmatched() -> Self {
        Self { rm_so: -1, rm_eo: -1 }
    }

    /// Converts a PCRE2 offset pair to an optional Rust byte range.
    fn to_offsets(self) -> Option<(usize, usize)> {
        let start = usize::try_from(self.rm_so).ok()?;
        let end = usize::try_from(self.rm_eo).ok()?;
        Some((start, end))
    }
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
