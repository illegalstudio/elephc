//! Purpose:
//! Coordinates ordinary and case-insensitive mbregex searches with a caller-owned capture output.
//!
//! Called from:
//! - The regex_request integration tests using independent PHP capture traces.
//!
//! Key details:
//! - An empty pattern fails before output initialization; other failures leave an initialized output.
//! - Initialization may enter PHP, so encoding, defaults, and search limits are read afterwards.

use super::{Event, Match, Session};
use crate::{error::MbError, regex::{Limits, RegexError, Subject}};
use std::rc::Rc;

impl Session {
    /// Searches once after protected output initialization; false from initialization stops the operation.
    /// A pending destructor exception can still allow searching when initialization completed successfully.
    /// Callers materialize successful captures with `Match::registers(false)` and retain pending exceptions.
    pub fn capture(&self, pattern: &[u8], subject: &[u8], ignore_case: bool,
        initialize: impl FnOnce() -> bool, limits: impl FnOnce() -> Limits,
        emit: &mut impl FnMut(Event)) -> Result<Option<Match>, RegexError> {
        let function = if ignore_case { "mb_eregi" } else { "mb_ereg" };
        if pattern.is_empty() {
            emit(Event::Exception(MbError::empty(function, 1, "pattern")));
            return Ok(None);
        }
        if !initialize() || !self.encoding().is_valid(subject) { return Ok(None); }
        let mut options = self.options();
        options.bits |= u32::from(ignore_case);
        let Some(regex) = self.compile(pattern, options, function, emit)? else { return Ok(None); };
        let subject = Rc::new(Subject::new(subject));
        match regex.search_subject(&subject, 0, false, limits()) {
            Ok(Some(captures)) => Ok(Some(Match { subject, captures })),
            Ok(None) | Err(RegexError::Search(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
