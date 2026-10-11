//! Purpose:
//! Owns raw mbstring INI values, startup defaults, and their changes to live request settings.
//!
//! Called from:
//! - Shared configuration adapters and the mb_language setting operation.
//!
//! Key details:
//! - Raw local/global values remain distinct from resolved encodings and active detection order.
//! - Failed writes still mark a directive modified; restore reruns its original-value handler.
//! - MIME validation is a supplied PCRE2 operation with no PHP callbacks or request mutation.

mod catalog;
mod handlers;
pub(super) mod numeric;
mod mutation;
mod string;
mod request;
pub use string::IniString;
pub use request::IniRequest;

use std::cell::RefCell;
use super::{State, OutputEncoding};
use crate::{arrays::{ArrayGraph, Key as ArrayKey, Value}, coercion::Diagnostic, encoding::Encoding};
use catalog::{Key, DIRECTIVES, REGISTRATION_ORDER};

/// Resolved core encoding defaults supplied by the host's core INI configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreEncodingDefaults { pub internal: Vec<u8>, pub input: Vec<u8>, pub output: Vec<u8> }

impl Default for CoreEncodingDefaults {
    /// Supplies the PHP baseline effective core encoding for all three inheritance paths.
    fn default() -> Self { Self { internal: b"UTF-8".to_vec(), input: b"UTF-8".to_vec(), output: b"UTF-8".to_vec() } }
}

/// A PCRE2 compile failure before an output MIME expression can replace the active setting.
#[derive(Debug, PartialEq, Eq)]
pub struct MimeRegexError { pub offset: u64, pub message: Vec<u8> }

/// A completed directive operation, including false-return state changes and ordered diagnostics.
#[derive(Debug, PartialEq, Eq)]
pub struct IniUpdate { pub accepted: bool, pub previous: Option<IniString>, pub diagnostics: Vec<Diagnostic> }

/// Raw values and the modified bit used by PHP's restore handler.
#[derive(Clone)]
pub(super) struct Entry {
    global: Option<IniString>, local: Option<IniString>, original: Option<IniString>,
    modified: bool, access: u8, original_access: u8,
}

impl Entry {
    /// Saves the actual prior raw identity once, before any warning handler can reenter the setting.
    fn mark_modified(&mut self) {
        if !self.modified { self.original = self.local.clone(); self.original_access = self.access; self.modified = true; }
    }
}

/// Configuration metadata whose lifetime is the same as the engine's request settings.
#[derive(Clone)]
pub(super) struct IniData {
    entries: [Entry; DIRECTIVES.len()],
    defaults: CoreEncodingDefaults,
    startup_defaults: CoreEncodingDefaults,
    configured_detect: Option<Vec<Encoding>>,
    pub(super) explicit_internal: bool,
    pub(super) explicit_output: bool,
    explicit_input: bool,
    regex_stack: i64,
    regex_retry: i64,
    regex_encoding: Vec<u8>,
}

impl Default for IniData {
    /// Creates raw PHP defaults without pretending nullable text directives were explicitly set.
    fn default() -> Self {
        Self { entries: std::array::from_fn(|index| {
            let value: Option<IniString> = DIRECTIVES[index].default.map(|value| IniString::interned(value.as_bytes()));
            Entry { global: value.clone(), local: value, original: None, modified: false, access: DIRECTIVES[index].access, original_access: 0 }
        }), defaults: CoreEncodingDefaults::default(), startup_defaults: CoreEncodingDefaults::default(), configured_detect: None, explicit_internal: false,
            explicit_output: false, explicit_input: false, regex_stack: 100_000, regex_retry: 1_000_000,
            regex_encoding: b"UTF-8".to_vec() }
    }
}

impl State {
    /// Reads raw local text, using an empty string for a registered null default and None for unknown keys.
    pub fn ini_get(&self, name: &[u8]) -> Option<&[u8]> {
        Some(self.ini.entries[catalog::lookup(name)? as usize].local.as_deref().unwrap_or_default())
    }

    /// Retains a scalar getter result with PHP's short-string identity normalization.
    pub fn ini_get_string(&self, name: &[u8]) -> Option<IniString> {
        Some(self.ini.entries[catalog::lookup(name)? as usize].local.as_ref()
            .map_or_else(|| IniString::interned(b""), IniString::scalar_result))
    }

    /// Retains raw local or displayed global text for ini_get_all, including nullable defaults.
    pub fn ini_get_all_string(&self, name: &[u8], global: bool) -> Option<IniString> {
        let entry = &self.ini.entries[catalog::lookup(name)? as usize];
        if global && entry.original.is_some() { entry.original.clone() } else { entry.local.clone() }
    }

    /// Preserves sorted keys and null local defaults, including PHP's local fallback for null global values.
    pub fn ini_get_all(&self, details: bool) -> ArrayGraph { self.ini_get_all_strings(details).0 }

    /// Returns the ordinary graph together with retained raw identities for every string-valued cell.
    pub fn ini_get_all_strings(&self, details: bool) -> (ArrayGraph, Vec<(usize, usize, IniString)>) {
        let mut strings = Vec::new();
        let mut arrays = vec![Vec::new()];
        for (index, directive) in DIRECTIVES.iter().enumerate() {
            let entry = &self.ini.entries[index];
            let mut raw = |value: &Option<IniString>, array, entry| value.as_ref().map_or(Value::Null, |value| {
                strings.push((array, entry, value.clone()));
                Value::String(value.to_vec())
            });
            let value = if details {
                let child = arrays.len();
                let global = if entry.original.is_some() { &entry.original } else { &entry.local };
                arrays.push(vec![(ArrayKey::String(b"global_value".to_vec()), raw(global, child, 0)),
                    (ArrayKey::String(b"local_value".to_vec()), raw(&entry.local, child, 1)),
                    (ArrayKey::String(b"access".to_vec()), Value::Int(entry.access as i64))]);
                Value::Array(child)
            } else { raw(&entry.local, 0, index) };
            arrays[0].push((ArrayKey::String(directive.name.as_bytes().to_vec()), value));
        }
        (ArrayGraph::new(0, arrays).expect("unique directive keys and completed detail children"), strings)
    }

    /// Applies a runtime string value after host coercion and returns the old raw value on success.
    pub fn ini_set(&mut self, name: &[u8], value: &[u8], mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>) -> IniUpdate {
        let mut diagnostics = Vec::new();
        let mut result = mutation::set(&mut &mut *self, name, IniString::new(value), &mut validate, &mut |warning| diagnostics.push(warning));
        result.diagnostics = diagnostics;
        result
    }

    /// Restores a modified directive through its startup-value handler, preserving any failure side effects.
    pub fn ini_restore(&mut self, name: &[u8], mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>) -> IniUpdate {
        let mut diagnostics = Vec::new();
        let mut result = mutation::restore(&mut &mut *self, name, &mut validate, &mut |warning| diagnostics.push(warning));
        result.diagnostics = diagnostics;
        result
    }

    /// Emits warnings at PHP's mutation points without retaining a request borrow during callbacks.
    /// Hosts contain PHP exceptions, record pending status, and return so the INI handler can finish.
    /// Diagnostics are already delivered and are not repeated in the returned update.
    pub fn ini_set_reentrant(state: &RefCell<Self>, name: &[u8], value: &[u8],
        mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>, mut emit: impl FnMut(Diagnostic)) -> IniUpdate {
        mutation::set(&mut &*state, name, IniString::new(value), &mut validate, &mut emit)
    }

    /// Sets already retained PHP text, preserving identity shared with a getter or earlier argument.
    pub fn ini_set_string_reentrant(state: &RefCell<Self>, name: &[u8], value: IniString,
        mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>, mut emit: impl FnMut(Diagnostic)) -> IniUpdate {
        mutation::set(&mut &*state, name, value, &mut validate, &mut emit)
    }

    /// Restores through the same handler while permitting nested INI and mbstring calls from diagnostics.
    /// Pending PHP exceptions do not unwind this Rust call; hosts preserve them until completion.
    pub fn ini_restore_reentrant(state: &RefCell<Self>, name: &[u8],
        mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>, mut emit: impl FnMut(Diagnostic)) -> IniUpdate {
        mutation::restore(&mut &*state, name, &mut validate, &mut emit)
    }

    /// Builds startup state in PHP directive-registration order, applying the final override for each key.
    pub fn with_ini_configuration(overrides: &[(Vec<u8>, Vec<u8>)], defaults: CoreEncodingDefaults,
        mut validate: impl FnMut(&[u8]) -> Result<(), MimeRegexError>) -> (Self, Vec<Diagnostic>) {
        let mut state = Self::default();
        let (core_ini, mut diagnostics) = super::CoreIni::with_overrides(overrides);
        state.core_ini = core_ini;
        state.response = super::Response::with_overrides(overrides);
        state.output = OutputEncoding::Pass;
        state.http_input_encodings.clear();
        state.ini.defaults = defaults.clone();
        state.ini.startup_defaults = defaults;
        for key in REGISTRATION_ORDER {
            let directive = &DIRECTIVES[key as usize];
            let configured = overrides.iter().rev().find(|(name, _)| name == directive.name.as_bytes()).map(|(_, value)| value.as_slice());
            let default = directive.default.map(str::as_bytes);
            let mut selected = configured.or(default);
            if !handlers::apply(&mut &mut state, key, selected, "PHP Startup", &mut validate, &mut |warning| diagnostics.push(warning)) {
                selected = default;
                handlers::apply(&mut &mut state, key, default, "PHP Startup", &mut validate, &mut |warning| diagnostics.push(warning));
            }
            let entry = &mut state.ini.entries[key as usize];
            entry.global = selected.map(IniString::interned);
            entry.local = entry.global.clone();
        }
        handlers::inherited(&mut &mut state, "PHP Startup", &mut |warning| diagnostics.push(warning));
        state.detect_order = state.ini.configured_detect.clone().unwrap_or_else(|| state.auto_language.detect_order());
        (state, diagnostics)
    }

    /// Reconstructs a request from validated startup values, clearing live mutations without recompiling MIME patterns.
    pub fn reset_ini_request(&mut self) {
        let overrides: Vec<_> = self.ini.entries.iter().zip(&DIRECTIVES).filter_map(|(entry, directive)|
            entry.global.as_ref().map(|value| (directive.name.as_bytes().to_vec(), value.to_vec()))).collect();
        let defaults = self.ini.startup_defaults.clone();
        let mut core_ini = self.core_ini.clone();
        core_ini.reset();
        let response = self.response.startup_reset();
        *self = Self::with_ini_configuration(&overrides, defaults, |_| Ok(())).0;
        self.core_ini = core_ini;
        self.response = response;
    }

    /// Updates inherited effective encodings while preserving explicit public/INI overrides and lookup caches.
    pub fn update_core_encoding_defaults(&mut self, defaults: CoreEncodingDefaults) -> Vec<Diagnostic> {
        self.ini.defaults = defaults;
        let mut diagnostics = Vec::new();
        handlers::inherited(&mut &mut *self, "ini_set", &mut |warning| diagnostics.push(warning));
        diagnostics
    }

    /// Applies a host core-encoding change with short state borrows around ordered warning callbacks.
    pub fn update_core_encoding_defaults_reentrant(state: &RefCell<Self>, defaults: CoreEncodingDefaults, mut emit: impl FnMut(Diagnostic)) {
        state.borrow_mut().ini.defaults = defaults;
        handlers::inherited(&mut &*state, "ini_set", &mut emit);
    }

    /// Returns the parsed signed INI limits for the shared regex engine to apply at match time.
    pub fn ini_regex_limits(&self) -> (i64, i64) { (self.ini.regex_stack, self.ini.regex_retry) }

    /// Retains the exact resolved INI handler input independently of public internal-encoding changes.
    pub fn ini_regex_encoding(&self) -> &[u8] { &self.ini.regex_encoding }

    /// Marks the public mb_language setter as an INI mutation before its handler can fail.
    pub(super) fn begin_language_ini_change(&mut self) { self.ini.entries[Key::Language as usize].mark_modified(); }

    /// Commits the exact mb_language argument spelling after its shared validation succeeds.
    pub(super) fn commit_language_ini_change(&mut self, name: &[u8]) { self.ini.entries[Key::Language as usize].local = Some(IniString::new(name)); }
}
