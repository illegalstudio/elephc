//! Purpose:
//! Applies shared INI handlers to a live text request and its thread-confined regex session.
//!
//! Called from:
//! - Protected native/eval INI adapters after argument coercion.
//!
//! Key details:
//! - Regex encoding changes occur after internal-encoding diagnostics and before later INI handlers.
//! - Neither text state nor regex state stays borrowed while PHP warning callbacks execute.

use super::*;
use mutation::Access;
use crate::regex::Session;

/// Couples short text-state borrows with regex configuration at the shared handler's commit point.
pub struct IniRequest<'a> { pub state: &'a RefCell<State>, pub regex: &'a Session }

impl Access for IniRequest<'_> {
    /// Releases the text request borrow before the handler emits diagnostics or configures regex state.
    fn with<T>(&mut self, action: impl FnOnce(&mut State) -> T) -> T { action(&mut self.state.borrow_mut()) }

    /// Applies the original encoding alias to the regex default and current validator.
    fn configure_regex(&mut self, name: &[u8]) { self.regex.configure_encoding(name); }
}

impl IniRequest<'_> {
    /// Sets retained INI text using the same reentrant mutation and identity rules as ordinary requests.
    pub fn set(&mut self, name: &[u8], value: IniString,
        mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>, mut emit: impl FnMut(Diagnostic)) -> IniUpdate {
        mutation::set(self, name, value, &mut validate, &mut emit)
    }

    /// Restores a modified directive and updates regex state at its actual encoding-handler boundary.
    pub fn restore(&mut self, name: &[u8],
        mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>, mut emit: impl FnMut(Diagnostic)) -> IniUpdate {
        mutation::restore(self, name, &mut validate, &mut emit)
    }

    /// Applies inherited core encodings while respecting flags changed by preceding warning callbacks.
    pub fn update_core_encoding_defaults(&mut self, defaults: CoreEncodingDefaults, mut emit: impl FnMut(Diagnostic)) {
        self.state.borrow_mut().ini.defaults = defaults;
        handlers::inherited(self, "ini_set", &mut emit);
    }
}
