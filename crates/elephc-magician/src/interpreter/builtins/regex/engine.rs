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

use std::ffi::{c_int, c_void, CStr, CString};
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
const REG_NOMATCH: c_int = 17;

/// Supported PHP regex modifiers after delimiter stripping.
#[derive(Default)]
pub(in crate::interpreter) struct EvalPregModifiers {
    pub(in crate::interpreter) case_insensitive: bool,
    pub(in crate::interpreter) multi_line: bool,
    pub(in crate::interpreter) dot_matches_new_line: bool,
    pub(in crate::interpreter) swap_greed: bool,
    pub(in crate::interpreter) unicode: bool,
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
        let pattern = CString::new(body).map_err(|_| EvalStatus::RuntimeFatal)?;
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

    /// Returns the number of capture slots including the full match at index 0.
    pub(in crate::interpreter) fn captures_len(&self) -> usize {
        self.capture_slots
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
        let Ok(subject_c) = CString::new(subject) else { return captures.into_iter(); };
        let mut continuation = false;
        loop {
            let Some(next) = self.exec_subject(subject, &subject_c, 0, continuation) else {
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
        let subject_c = CString::new(subject).ok()?;
        self.exec_subject(subject, &subject_c, start, false)
    }

    /// Executes with stable C-string storage while preserving the original Rust subject lifetime.
    fn exec_subject<'a>(
        &self, subject: &'a [u8], subject_c: &CStr, start: usize, continuation: bool,
    ) -> Option<Captures<'a>> {
        let mut offset_pairs = vec![-1_i64; self.captures_len().checked_mul(2)?];
        offset_pairs[0] = i64::try_from(start).ok()?;
        offset_pairs[1] = i64::try_from(subject.len()).ok()?;
        let status = unsafe {
            (self.provider.exec)(
                self.handle,
                subject_c.as_ptr(),
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
