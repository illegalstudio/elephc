//! Purpose:
//! Coordinates mbregex matching and progressive search with PHP's observable mutation order.
//!
//! Called from:
//! - Shared request adapters after native/eval argument coercion has completed.
//!
//! Key details:
//! - Progressive option errors retain partial syntax and do not abort subsequent state changes.
//! - Diagnostic callbacks run without request borrows and can replace any live search setting.

use super::{warning, Event, Match, Session};
use crate::error::MbError;
use crate::regex::{Limits, Options, RegexError, Subject};
use std::rc::Rc;

impl Session {
    /// Validates and matches an anchored pattern with explicit or live default options.
    pub fn is_match(&self, pattern: &[u8], subject: &[u8], options: Option<&[u8]>, limits: Limits,
        emit: &mut impl FnMut(Event)) -> Result<bool, RegexError> {
        let options = match options {
            Some(input) => match Options::parse(input, false) {
                Ok(options) => options, Err(error) => { emit(Event::Exception(error)); return Ok(false); }
            },
            None => self.options(),
        };
        if !self.encoding().is_valid(subject) { return Ok(false); }
        let Some(regex) = self.compile(pattern, options, "mb_ereg_match", emit)? else { return Ok(false); };
        match regex.search(subject, 0, true, limits) {
            Ok(result) => Ok(result.is_some()), Err(RegexError::Search(_)) => Ok(false), Err(error) => Err(error),
        }
    }

    /// Initializes a retained subject after optional compilation, preserving the old subject on compile failure.
    pub fn initialize(&self, subject: &[u8], pattern: Option<&[u8]>, options: Option<&[u8]>,
        emit: &mut impl FnMut(Event)) -> Result<bool, RegexError> {
        if pattern == Some(b"") {
            emit(Event::Exception(MbError::empty("mb_ereg_search_init", 2, "pattern")));
            return Ok(false);
        }
        let options = self.progressive_options(options, emit);
        if let Some(pattern) = pattern {
            let regex = self.compile(pattern, options, "mb_ereg_search_init", emit)?;
            let found = regex.is_some();
            self.data.borrow_mut().pattern = regex;
            if !found { return Ok(false); }
        }
        let valid = self.encoding().is_valid(subject);
        let mut data = self.data.borrow_mut();
        data.subject = Some(Rc::new(Subject::new(subject)));
        data.position = if valid { 0 } else { subject.len() };
        data.matched = None;
        data.simple_pattern = false;
        Ok(valid)
    }

    /// Reinitializes the progressive subject for the dedicated `[0-9]+` path.
    ///
    /// Repeating `mb_ereg_search_init()` with the same subject, encoding, and options is the
    /// common request-local loop shape. The retained subject is already an owned copy, so the
    /// fast path only compares its bytes and resets the progressive cursor without recompiling
    /// the pattern or allocating a second subject.
    pub fn initialize_simple(&self, subject: &[u8]) -> Result<bool, RegexError> {
        {
            let mut data = self.data.borrow_mut();
            if data.simple_pattern
                && data
                    .subject
                    .as_ref()
                    .is_some_and(|retained| &retained[..] == subject)
            {
                data.position = 0;
                data.matched = None;
                return Ok(true);
            }
        }
        let valid = self.initialize(subject, Some(b"[0-9]+"), None, &mut |_| {})?;
        if valid {
            self.data.borrow_mut().simple_pattern = true;
        }
        Ok(valid)
    }

    /// Searches the retained subject and updates registers/position at PHP's callback-visible boundaries.
    pub fn search(&self, pattern: Option<&[u8]>, options: Option<&[u8]>, function: &str, limits: Limits,
        emit: &mut impl FnMut(Event)) -> Result<Option<Match>, RegexError> {
        let options = self.progressive_options(options, emit);
        self.data.borrow_mut().matched = None;
        if let Some(pattern) = pattern {
            let regex = self.compile(pattern, options, function, emit)?;
            let found = regex.is_some();
            let mut data = self.data.borrow_mut();
            data.pattern = regex;
            data.simple_pattern = false;
            if !found { return Ok(None); }
        }
        let (pattern, subject, position) = {
            let data = self.data.borrow();
            (data.pattern.clone(), data.subject.clone(), data.position)
        };
        let Some(pattern) = pattern else {
            emit(Event::Exception(MbError::Runtime("No pattern was provided".into())));
            return Ok(None);
        };
        let Some(subject) = subject else {
            emit(Event::Exception(MbError::Runtime("No string was provided".into())));
            return Ok(None);
        };
        match pattern.search_subject(&subject, position, false, limits) {
            Ok(Some(captures)) => {
                let end = captures.groups.first().and_then(Option::as_ref).ok_or(RegexError::Provider)?.end;
                let matched = Match { subject, captures };
                let mut data = self.data.borrow_mut();
                data.position = if position <= end { end } else { position + 1 };
                data.matched = Some(matched.clone());
                Ok(Some(matched))
            }
            Ok(None) => {
                let mut data = self.data.borrow_mut();
                data.position = subject.len();
                data.matched = None;
                Ok(None)
            }
            Err(RegexError::Search(message)) => {
                let mut bytes = b"mbregex search failure in mbregex_search(): ".to_vec();
                bytes.extend(message);
                warning(function, &bytes, emit);
                self.data.borrow_mut().matched = None;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Publishes a progressive parser exception immediately while retaining PHP's partial syntax result.
    fn progressive_options(&self, input: Option<&[u8]>, emit: &mut impl FnMut(Event)) -> Options {
        match input {
            None => self.options(),
            Some(input) => {
                let (options, error) = Options::parse_progressive(input, false);
                if let Some(error) = error { emit(Event::Exception(error)); }
                options
            }
        }
    }
}
