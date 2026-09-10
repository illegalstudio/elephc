//! Purpose:
//! Splits multibyte subjects with PHP's limit, empty-match, and diagnostic rules.
//!
//! Called from:
//! - The shared mb_split dispatcher after argument coercion.
//!
//! Key details:
//! - Subject validation precedes compilation even when the limit suppresses searching.
//! - Empty matches advance one byte while the output cursor preserves all unmatched bytes.

use super::{warning, Event, Session};
use crate::regex::{native_error, Limits, RegexError, Subject};

impl Session {
    /// Returns independent split fields or false, sharing compiled-pattern cache side effects with other calls.
    pub fn split(&self, pattern: &[u8], subject: &[u8], limit: i64, limits: Limits,
        emit: &mut impl FnMut(Event)) -> Result<Option<Vec<Vec<u8>>>, RegexError> {
        if !self.encoding().is_valid(subject) { return Ok(None); }
        let Some(regex) = self.compile(pattern, self.options(), "mb_split", emit)? else { return Ok(None); };
        let subject = Subject::new(subject);
        let mut remaining = (limit as u64).saturating_sub(1);
        let (mut position, mut chunk) = (0, 0);
        let mut fields = Vec::new();
        while remaining != 0 && position < subject.len() {
            let captures = match regex.search_subject(&subject, position, false, limits) {
                Ok(Some(captures)) => captures,
                Ok(None) => break,
                Err(RegexError::Search(message)) => { split_warning(&message, emit); return Ok(None); },
                Err(error) => return Err(error),
            };
            let matched = captures.groups.first().and_then(Option::as_ref).ok_or(RegexError::Provider)?;
            if position < matched.end {
                if matched.start >= subject.len() || matched.start < chunk {
                    split_warning(&native_error(regex.provider, -2)?, emit);
                    return Ok(None);
                }
                fields.push(subject[chunk..matched.start].to_vec());
                remaining -= 1;
                chunk = matched.end;
                position = matched.end;
            } else {
                position += 1;
            }
        }
        fields.push(subject[chunk..].to_vec());
        Ok(Some(fields))
    }
}

/// Delivers PHP's split-specific search warning after all request-state borrows have ended.
fn split_warning(message: &[u8], emit: &mut impl FnMut(Event)) {
    let mut bytes = b"mbregex search failure in mbsplit(): ".to_vec();
    bytes.extend_from_slice(message);
    warning("mb_split", &bytes, emit);
}
