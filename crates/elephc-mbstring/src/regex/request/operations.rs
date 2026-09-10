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
            self.data.borrow_mut().pattern = regex;
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
