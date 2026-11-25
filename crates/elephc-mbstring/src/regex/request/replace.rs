//! Purpose:
//! Applies ordinary and callback mbregex replacements through one shared search loop.
//!
//! Called from:
//! - Shared replacement dispatch after protected argument and callback conversion.
//!
//! Key details:
//! - Subject validation and replacement scanning use the encoding captured before PHP coercions.
//! - Pattern compilation uses live settings; empty matches advance by one subject byte.

mod walk;

use std::convert::Infallible;
use super::{Event, Session};
use crate::regex::{Captures, Limits, RegexEncoding, RegexError};
pub use walk::{CallbackReplacement, ReplacementError};

/// Already converted inputs retain the entry encoding separately from the live compilation encoding.
pub struct Replacement<'a> {
    pub pattern: &'a [u8],
    pub replacement: &'a [u8],
    pub subject: &'a [u8],
    pub options: Option<&'a [u8]>,
    pub encoding: RegexEncoding,
    pub ignore_case: bool,
}

/// PHP distinguishes invalid subjects (null) from regex failures (false) and successful strings.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplaceResult { InvalidSubject, Failed, String(Vec<u8>) }

impl Session {
    /// Replaces every match while preserving PHP option errors, warning reentry, and cache mutations.
    pub fn replace(&self, input: Replacement<'_>, limits: Limits,
        emit: &mut impl FnMut(Event)) -> Result<ReplaceResult, RegexError> {
        let function = if input.ignore_case { "mb_eregi_replace" } else { "mb_ereg_replace" };
        let search = CallbackReplacement { pattern: input.pattern, subject: input.subject,
            options: input.options, encoding: input.encoding };
        let result = walk::replace_with(self, search, function, input.ignore_case, || limits, emit,
            |regex| {
                let numbered = regex.numbered_backrefs()?;
                let mut replacement = input.replacement.to_vec();
                // Initialize lookahead, including the string terminator consumed by incomplete backreferences.
                replacement.resize(replacement.len() + 4, 0);
                Ok(move |matched: &crate::regex::Match, output: &mut Vec<u8>| -> Result<(), ReplacementError<Infallible>> {
                    substitute(output, &matched.subject, &replacement, input.replacement.len(),
                        input.encoding, &matched.captures, numbered);
                    Ok(())
                })
            });
        match result {
            Ok(result) => Ok(result),
            Err(ReplacementError::Regex(error)) => Err(error),
            Err(ReplacementError::Callback(error)) => match error {},
        }
    }
}

/// Expands PHP backreferences without treating doubled backslashes or dollar signs as escapes.
fn substitute(output: &mut Vec<u8>, subject: &[u8], replacement: &[u8], length: usize,
    encoding: RegexEncoding, captures: &Captures, numbered: bool) {
    let mut position = 0;
    while position < length {
        let start = position;
        let width = encoding.replacement_width(replacement[position]);
        if width != 1 || replacement[position] != b'\\' {
            // A truncated multibyte sequence at the end of the replacement still reports its
            // expected width. Copy only the bytes that belong to the replacement: PHP 8.5's
            // mb_regex_substitute() reads source length + width and exposes the terminator.
            let end = position.saturating_add(width).min(length);
            output.extend_from_slice(&replacement[start..end]);
            position = end;
            continue;
        }
        position += 1;
        if position == length || encoding.replacement_width(replacement[position]) != 1 {
            output.extend_from_slice(&replacement[start..position]);
            continue;
        }
        let group = match replacement[position] {
            b'0' => { position += 1; Some(0) },
            b'1'..=b'9' => { let group = usize::from(replacement[position] - b'0'); position += 1; numbered.then_some(group) },
            b'k' => {
                position += 1;
                let width = encoding.replacement_width(replacement[position]);
                if width != 1 || position == length || !matches!(replacement[position], b'<' | b'\'') {
                    let end = position.saturating_add(width).min(length);
                    output.extend_from_slice(&replacement[start..end]);
                    position = end;
                    continue;
                }
                let delimiter = if replacement[position] == b'<' { b'>' } else { b'\'' };
                let name = position + 1;
                let mut end = name;
                let mut numeric = true;
                while end < length {
                    let width = encoding.replacement_width(replacement[end]);
                    if width != 1 { end += width; numeric = false; continue; }
                    if replacement[end] == delimiter { break; }
                    numeric &= replacement[end].is_ascii_digit();
                    end += 1;
                }
                // An unterminated group name may run past `length` by one multibyte width;
                // clamp the emitted fallback to the real replacement instead of its padding.
                position = end.saturating_add(1).min(length);
                if end == name || end >= length { None }
                else if numeric { numbered.then(|| numbered_group(&replacement[name..end])).flatten() }
                else { captures.names.iter().find(|(key, _)| key == &replacement[name..end]).map(|(_, group)| *group) }
            },
            _ => { output.extend_from_slice(&replacement[start..position]); continue; },
        };
        match group.and_then(|group| captures.groups.get(group)) {
            Some(Some(range)) => output.extend_from_slice(&subject[range.clone()]),
            Some(None) => {},
            None => output.extend_from_slice(&replacement[start..position]),
        }
    }
}

/// Applies PHP's leading-zero rejection and rejects group numbers above its signed-int range.
fn numbered_group(bytes: &[u8]) -> Option<usize> {
    if bytes.len() > 1 && bytes[0] == b'0' { return None; }
    let number = bytes.iter().fold(0u64, |value, byte| value.saturating_mul(10).saturating_add(u64::from(byte - b'0')));
    // PHP's `(int) strtoul(...)` wraps 2^32+1 back to group 1. Keep the escape literal
    // instead of silently addressing an unrelated group.
    usize::try_from(number).ok().filter(|value| *value <= i32::MAX as usize)
}
