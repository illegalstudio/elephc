//! Purpose:
//! Owns mbregex request settings, compiled-pattern caching, and progressive search storage.
//!
//! Called from:
//! - Shared regex operation coordinators and native/eval request adapters.
//!
//! Key details:
//! - Cache replacement invalidates a dependent progressive pattern and its captures.
//! - No state borrow survives a PHP diagnostic callback; active patterns have independent owners.

mod operations;
mod split;
mod replace;
mod capture;
pub use replace::{CallbackReplacement, Replacement, ReplacementError, ReplaceResult};

use std::{cell::RefCell, collections::HashMap, rc::Rc};
use crate::error::{MbError, MbResult};
use super::{Captures, Options, Regex, RegexEncoding, RegexError, Subject};

/// Ordered PHP-visible events, emitted at their actual state-transition boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event { Warning(Vec<u8>), Exception(MbError) }

/// Integer capture indexes and exact byte-valued names retain their PHP array key types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisterKey { Index(usize), Name(Vec<u8>) }

/// A false register is distinct from a matched empty byte string.
pub type Registers = Vec<(RegisterKey, Option<Vec<u8>>)>;

/// An independently retained subject and copied captures survive cache changes and callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match { pub subject: Rc<Subject>, pub captures: Captures }

impl Match {
    /// Materializes numeric groups with the operation's empty policy; named empty groups are always false.
    pub fn registers(&self, keep_empty: bool) -> Registers {
        let value = |index: usize, empty| self.captures.groups[index].as_ref()
            .filter(|range| empty || !range.is_empty()).map(|range| self.subject[range.clone()].to_vec());
        let mut result = (0..self.captures.groups.len())
            .map(|index| (RegisterKey::Index(index), value(index, keep_empty))).collect::<Vec<_>>();
        result.extend(self.captures.names.iter().map(|(name, index)| (RegisterKey::Name(name.clone()), value(*index, false))));
        result
    }

    /// Returns the complete match's byte offset and length after native capture validation.
    pub fn position(&self) -> (usize, usize) {
        let range = self.captures.groups[0].as_ref().expect("validated full regex match");
        (range.start, range.len())
    }
}

/// A byte-keyed entry compares native encoding identity, not the alias used by PHP validation.
struct Cached { encoding: u32, options: Options, regex: Rc<Regex> }

/// Mutable storage stays private so callbacks can reenter through short, independent borrows.
#[derive(Default)]
struct Data {
    encoding: RegexEncoding,
    default_encoding: RegexEncoding,
    options: Options,
    cache: HashMap<Vec<u8>, Cached>,
    subject: Option<Rc<Subject>>,
    position: usize,
    pattern: Option<Rc<Regex>>,
    matched: Option<Match>,
}

/// One thread-confined mbregex session shared by every operation in a request.
#[derive(Default)]
pub struct Session { data: RefCell<Data> }

impl Session {
    /// Returns the canonical current encoding without modifying its alias-sensitive validator.
    pub fn encoding(&self) -> RegexEncoding { self.data.borrow().encoding }

    /// Changes the live validation alias only after both PHP and Oniguruma accept its spelling.
    pub fn set_encoding(&self, name: &[u8]) -> MbResult<()> {
        let encoding = RegexEncoding::lookup(name).ok_or_else(||
            MbError::argument_value("mb_regex_encoding", 1, "encoding", "must be a valid encoding, ", name, " given"))?;
        self.data.borrow_mut().encoding = encoding;
        Ok(())
    }

    /// Applies the resolved internal-encoding INI handler input after its ordinary mbstring diagnostics.
    /// Unsupported regex names reset the native default to UTF-8 but retain the current encoding.
    pub fn configure_encoding(&self, name: &[u8]) {
        let mut data = self.data.borrow_mut();
        data.default_encoding = RegexEncoding::default_for_ini(name).unwrap_or_default();
        if let Some(encoding) = RegexEncoding::lookup(name) { data.encoding = encoding; }
    }

    /// Returns the current default options without changing cached or progressive patterns.
    pub fn options(&self) -> Options { self.data.borrow().options }

    /// Publishes validated options and returns the previous default, as mb_regex_set_options does.
    pub fn set_options(&self, input: &[u8]) -> MbResult<Options> {
        let options = Options::parse(input, false)?;
        Ok(std::mem::replace(&mut self.data.borrow_mut().options, options))
    }

    /// Releases request-owned patterns, captures, and subjects while retaining PHP's worker option defaults.
    pub fn reset_request(&self) {
        let mut data = self.data.borrow_mut();
        *data = Data { options: data.options, encoding: data.default_encoding,
            default_encoding: data.default_encoding, ..Data::default() };
    }

    /// Returns the progressive byte position even before a subject has been initialized.
    pub fn position(&self) -> usize { self.data.borrow().position }

    /// Sets a byte position, resolving negatives only when an initialized subject supplies its length.
    pub fn set_position(&self, position: i64) -> MbResult<()> {
        let mut data = self.data.borrow_mut();
        let length = data.subject.as_ref().map(|subject| subject.len());
        let resolved = if position < 0 { length.and_then(|length| (length as i64).checked_add(position)) } else { Some(position) };
        let position = resolved.and_then(|position| usize::try_from(position).ok())
            .filter(|&position| length.is_none_or(|length| position <= length))
            .ok_or_else(|| MbError::argument("mb_ereg_search_setpos", 1, "offset", "is out of range"))?;
        data.position = position;
        Ok(())
    }

    /// Copies retained register values without consuming them or changing the current position.
    pub fn registers(&self) -> Option<Registers> { self.data.borrow().matched.as_ref().map(|matched| matched.registers(true)) }

    /// Validates every lookup with the current alias, reusing only the same native pattern profile.
    pub fn compile(&self, pattern: &[u8], options: Options, function: &str,
        emit: &mut impl FnMut(Event)) -> Result<Option<Rc<Regex>>, RegexError> {
        let encoding = self.encoding();
        if encoding.is_valid(pattern) {
            let data = self.data.borrow();
            if let Some(cached) = data.cache.get(pattern) {
                if cached.encoding == encoding.id && cached.options == options { return Ok(Some(cached.regex.clone())); }
            }
        }
        match Regex::compile(pattern, encoding, options) {
            Ok(regex) => {
                let options = Options { bits: regex.compiled_options()?, ..options };
                let regex = Rc::new(regex);
                let mut data = self.data.borrow_mut();
                if let Some(previous) = data.cache.get(pattern) {
                    if data.pattern.as_ref().is_some_and(|current| Rc::ptr_eq(current, &previous.regex)) {
                        data.pattern = None;
                        data.matched = None;
                    }
                }
                data.cache.insert(pattern.to_vec(), Cached { encoding: encoding.id, options, regex: regex.clone() });
                Ok(Some(regex))
            }
            Err(RegexError::Pattern(message)) => { warning(function, &message, emit); Ok(None) }
            Err(error) => Err(error),
        }
    }
}

/// Prefixes one native diagnostic with its active public PHP function name outside any state borrow.
fn warning(function: &str, message: &[u8], emit: &mut impl FnMut(Event)) {
    let mut bytes = format!("{function}(): ").into_bytes();
    bytes.extend_from_slice(message);
    emit(Event::Warning(bytes));
}
