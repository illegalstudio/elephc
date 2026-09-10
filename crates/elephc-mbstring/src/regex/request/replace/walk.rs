//! Purpose:
//! Shares replacement validation, matching, and byte advancement across literal and callback forms.
//!
//! Called from:
//! - `Session::replace()` and `Session::replace_callback()` after ordered host argument preparation.
//!
//! Key details:
//! - Active patterns and subjects outlive callbacks independently of request settings and cache entries.
//! - Each search reads current limits; a callback error stops matching and discards partial output.

use std::rc::Rc;
use super::ReplaceResult;
use super::super::warning;
use crate::regex::{Event, Limits, Match, Options, Regex, RegexEncoding, RegexError, RegisterKey,
    Registers, Session, Subject};

/// Converted search inputs preserve the subject validation encoding captured on builtin entry.
pub struct CallbackReplacement<'a> {
    pub pattern: &'a [u8],
    pub subject: &'a [u8],
    pub options: Option<&'a [u8]>,
    pub encoding: RegexEncoding,
}

/// Keeps host callback failures separate from unavailable or malformed native regex integration.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplacementError<E> { Regex(RegexError), Callback(E) }

impl<E> From<RegexError> for ReplacementError<E> {
    /// Propagates native integration failures without interpreting them as callback exceptions.
    fn from(error: RegexError) -> Self { Self::Regex(error) }
}

impl Session {
    /// Replaces each match with already string-cast callback bytes and propagates callback failures.
    pub fn replace_callback<E>(&self, input: CallbackReplacement<'_>, limits: impl FnMut() -> Limits,
        emit: &mut impl FnMut(Event), mut callback: impl FnMut(Registers) -> Result<Vec<u8>, E>)
        -> Result<ReplaceResult, ReplacementError<E>> {
        replace_with(self, input, "mb_ereg_replace_callback", false, limits, emit, |_| Ok(move |matched: &Match, output: &mut Vec<u8>| {
            let mut registers = matched.registers(true);
            // PHP fills unmatched numeric groups with empty strings but keeps named empty groups false.
            for (key, value) in &mut registers {
                if matches!(key, RegisterKey::Index(_)) && value.is_none() { *value = Some(Vec::new()); }
            }
            let replacement = callback(registers).map_err(ReplacementError::Callback)?;
            output.extend(replacement);
            Ok(())
        }))
    }
}

/// Runs one replacement search, allowing each renderer to append bytes without duplicating regex rules.
pub(super) fn replace_with<E, R>(session: &Session, input: CallbackReplacement<'_>, function: &str,
    ignore_case: bool, mut limits: impl FnMut() -> Limits, emit: &mut impl FnMut(Event),
    prepare: impl FnOnce(&Regex) -> Result<R, ReplacementError<E>>)
    -> Result<ReplaceResult, ReplacementError<E>>
    where R: FnMut(&Match, &mut Vec<u8>) -> Result<(), ReplacementError<E>> {
    if !input.encoding.is_valid(input.subject) { return Ok(ReplaceResult::InvalidSubject); }
    let options = match input.options {
        Some(bytes) => match Options::parse(bytes, ignore_case) {
            Ok(options) => options,
            Err(error) => { emit(Event::Exception(error)); return Ok(ReplaceResult::Failed); },
        },
        None => { let mut options = session.options(); options.bits |= u32::from(ignore_case); options },
    };
    let Some(regex) = session.compile(input.pattern, options, function, emit)? else { return Ok(ReplaceResult::Failed); };
    let mut render = prepare(&regex)?;
    let subject = Rc::new(Subject::new(input.subject));
    let mut output = Vec::new();
    let mut position = 0;
    while position <= subject.len() {
        let captures = match regex.search_subject(&subject, position, false, limits()) {
            Ok(Some(captures)) => captures,
            Ok(None) => { output.extend_from_slice(&subject[position..]); break; },
            Err(RegexError::Search(message)) => {
                let mut bytes = b"mbregex search failure in php_mbereg_replace_exec(): ".to_vec();
                bytes.extend(message);
                warning(function, &bytes, emit);
                return Ok(ReplaceResult::Failed);
            },
            Err(error) => return Err(error.into()),
        };
        let range = captures.groups.first().and_then(Option::as_ref).ok_or(RegexError::Provider)?;
        if range.start < position { return Err(RegexError::Provider.into()); }
        let end = range.end;
        output.extend_from_slice(&subject[position..range.start]);
        render(&Match { subject: subject.clone(), captures }, &mut output)?;
        if position < end { position = end; }
        else {
            if position < subject.len() { output.push(subject[position]); }
            position += 1;
        }
    }
    Ok(ReplaceResult::String(output))
}
