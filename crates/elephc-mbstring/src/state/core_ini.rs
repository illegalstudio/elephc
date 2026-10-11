//! Purpose:
//! Owns the raw and parsed Core INI values used by mbstring query parsing.
//!
//! Called from:
//! - Shared request initialization, Core INI operations, and the V5 query provider.
//!
//! Key details:
//! - Startup validates quantities once; query callbacks read the resulting signed limits.
//! - Runtime changes respect access masks and preserve leased PHP string identities.
//! - Request reset restores the configured global values, including display_errors.

use elephc_builtin_contract::mbstring_abi::ini::core::{
    lookup, Key, DIRECTIVES, REGISTRATION_ORDER, SORTED_KEYS,
};
use crate::{arrays::{ArrayGraph, Key as ArrayKey, Value}, coercion::Diagnostic};
use super::{IniString, ini::numeric};

/// One request's raw Core settings and their already-validated parser values.
#[derive(Clone)]
pub struct CoreIni {
    global: [Option<IniString>; DIRECTIVES.len()],
    local: [Option<IniString>; DIRECTIVES.len()],
    max_variables: i64,
    max_nesting: i64,
    display_errors: u8,
}

impl Default for CoreIni {
    /// Installs PHP's scalar and null defaults before any startup overrides.
    fn default() -> Self {
        let global = std::array::from_fn(|index| {
            DIRECTIVES[index]
                .default
                .map(|value| IniString::interned(value.as_bytes()))
        });
        let max_variables = numeric::quantity(raw(&global, Key::MaxVariables)).0;
        let max_nesting = numeric::quantity(raw(&global, Key::MaxNesting)).0;
        let display_errors = numeric::display_errors(raw(&global, Key::DisplayErrors));
        Self { local: global.clone(), global, max_variables, max_nesting, display_errors }
    }
}

impl CoreIni {
    /// Applies final raw overrides in registration order, retaining defaults after rejected values.
    pub fn with_overrides(overrides: &[(Vec<u8>, Vec<u8>)]) -> (Self, Vec<Diagnostic>) {
        let mut state = Self::default();
        let mut diagnostics = Vec::new();
        for key in REGISTRATION_ORDER {
            let directive = &DIRECTIVES[key as usize];
            let Some((_, value)) = overrides.iter().rev().find(|(name, _)| name == directive.name.as_bytes()) else { continue; };
            let accepted = match key {
                Key::Separators => !value.is_empty(),
                Key::DisplayErrors => { state.display_errors = numeric::display_errors(value); true },
                Key::MaxNesting | Key::MaxVariables => {
                    let (parsed, warning) = numeric::quantity(value);
                    if let Some(warning) = warning {
                        let mut message = format!("Invalid \"{}\" setting. ", directive.name).into_bytes();
                        message.extend_from_slice(&warning);
                        diagnostics.push(Diagnostic { level: 2, message });
                    }
                    if parsed < 0 { false } else {
                        if key == Key::MaxNesting { state.max_nesting = parsed; } else { state.max_variables = parsed; }
                        true
                    }
                },
                Key::MailCrLfMode => {
                    if valid_cr_lf_mode(value) {
                        true
                    } else {
                        diagnostics.push(Diagnostic {
                            level: 1,
                            message: invalid_cr_lf_mode_message(value),
                        });
                        false
                    }
                },
                Key::Smtp
                | Key::SmtpPort
                | Key::SendmailFrom
                | Key::SendmailPath
                | Key::MailForceExtraParameters
                | Key::MailMixedLfAndCrlf => true,
            };
            if accepted {
                state.global[key as usize] = Some(IniString::interned(value));
                state.local[key as usize] = state.global[key as usize].clone();
            }
        }
        (state, diagnostics)
    }

    /// Returns one independently retained raw local string, or None for an unknown exact name.
    pub fn get(&self, name: &[u8]) -> Option<IniString> {
        let value = &self.local[lookup(name)? as usize];
        Some(value.as_ref().map_or_else(|| IniString::interned(b""), IniString::scalar_result))
    }

    /// Applies a permitted runtime setting and returns its previous exact string identity.
    pub fn set(&mut self, name: &[u8], value: IniString) -> Option<IniString> {
        let key = lookup(name)?;
        if DIRECTIVES[key as usize].access & 1 == 0 { return None; }
        if key == Key::MailCrLfMode && !valid_cr_lf_mode(&value) {
            return None;
        }
        if key == Key::DisplayErrors {
            self.display_errors = numeric::display_errors(&value);
        }
        let previous = std::mem::replace(&mut self.local[key as usize], Some(value));
        Some(previous.as_ref().map_or_else(|| IniString::interned(b""), IniString::scalar_result))
    }

    /// Restores a user-modifiable setting to its startup string and parsed value.
    pub fn restore(&mut self, name: &[u8]) {
        let Some(key) = lookup(name) else { return; };
        if DIRECTIVES[key as usize].access & 1 == 0 {
            return;
        }
        self.local[key as usize] = self.global[key as usize].clone();
        if key == Key::DisplayErrors {
            self.display_errors = numeric::display_errors(raw(&self.local, key));
        }
    }

    /// Restores request-local mutations while retaining validated startup limits and separators.
    pub fn reset(&mut self) {
        self.local = self.global.clone();
        self.display_errors = numeric::display_errors(raw(&self.local, Key::DisplayErrors));
    }

    /// Borrows the C-string separator bytes until the next request mutation or reset.
    pub fn separators(&self) -> &[u8] {
        raw(&self.local, Key::Separators).split(|byte| *byte == 0).next().unwrap()
    }

    /// Returns the validated whole-query input variable limit, including zero.
    pub fn max_variables(&self) -> i64 { self.max_variables }

    /// Returns the validated nesting limit read before each field registration.
    pub fn max_nesting(&self) -> i64 { self.max_nesting }

    /// Returns PHP's disabled, stdout, or stderr display mode without collapsing its raw INI value.
    pub fn display_errors(&self) -> u8 { self.display_errors }

    /// Returns the configured SMTP host as raw PHP bytes.
    pub fn smtp_host(&self) -> &[u8] { raw(&self.local, Key::Smtp) }

    /// Returns PHP's effective Windows SMTP port, including short truncation and zero fallback.
    pub fn smtp_port(&self) -> u16 {
        let port = numeric::quantity(raw(&self.local, Key::SmtpPort)).0 as i16 as u16;
        if port == 0 { 25 } else { port }
    }

    /// Returns the configured envelope sender, preserving a null default separately from empty.
    pub fn sendmail_from(&self) -> Option<&[u8]> { optional_raw(&self.local, Key::SendmailFrom) }

    /// Returns the selected process transport, null by default on Windows.
    pub fn sendmail_path(&self) -> Option<&[u8]> { optional_raw(&self.local, Key::SendmailPath) }

    /// Returns forced process-transport arguments, including an explicitly empty setting.
    pub fn mail_force_extra_parameters(&self) -> Option<&[u8]> {
        optional_raw(&self.local, Key::MailForceExtraParameters)
    }

    /// Returns PHP's legacy mail line-separator compatibility flag.
    pub fn mail_mixed_lf_and_crlf(&self) -> bool {
        numeric::boolean(raw(&self.local, Key::MailMixedLfAndCrlf))
    }

    /// Returns the validated raw `mail.cr_lf_mode` value.
    pub fn mail_cr_lf_mode(&self) -> &[u8] { raw(&self.local, Key::MailCrLfMode) }

    /// Builds a sorted Core directive graph with an identity owner for every string-valued cell.
    pub fn all(&self, details: bool) -> (ArrayGraph, Vec<(usize, usize, IniString)>) {
        let mut arrays = vec![Vec::new()];
        let mut strings = Vec::new();
        for (position, key) in SORTED_KEYS.into_iter().enumerate() {
            let index = key as usize;
            let directive = &DIRECTIVES[index];
            let mut materialize = |value: &Option<IniString>, array, entry| {
                value.as_ref().map_or(Value::Null, |value| {
                    strings.push((array, entry, value.clone()));
                    Value::String(value.to_vec())
                })
            };
            let value = if details {
                let child = arrays.len();
                arrays.push(vec![
                    (ArrayKey::String(b"global_value".to_vec()), materialize(&self.global[index], child, 0)),
                    (ArrayKey::String(b"local_value".to_vec()), materialize(&self.local[index], child, 1)),
                    (ArrayKey::String(b"access".to_vec()), Value::Int(directive.access as i64)),
                ]);
                Value::Array(child)
            } else { materialize(&self.local[index], 0, position) };
            arrays[0].push((ArrayKey::String(directive.name.as_bytes().to_vec()), value));
        }
        (ArrayGraph::new(0, arrays).expect("unique Core keys and complete detail children"), strings)
    }
}

/// Borrows a non-null directive value, whose catalog contract supplies a scalar default.
fn raw(values: &[Option<IniString>; DIRECTIVES.len()], key: Key) -> &[u8] {
    values[key as usize].as_deref().unwrap_or_default()
}

/// Borrows a directive while preserving its PHP null default.
fn optional_raw(values: &[Option<IniString>; DIRECTIVES.len()], key: Key) -> Option<&[u8]> {
    values[key as usize].as_deref()
}

/// Validates PHP 8.5's accepted mail line-ending policy spellings.
fn valid_cr_lf_mode(value: &[u8]) -> bool {
    value.is_empty()
        || [b"crlf".as_slice(), b"lf", b"mixed", b"os"]
            .iter()
            .any(|accepted| value == *accepted)
}

/// Formats PHP's startup-stage `mail.cr_lf_mode` validation diagnostic.
fn invalid_cr_lf_mode_message(value: &[u8]) -> Vec<u8> {
    let mut message = b"Invalid value \"".to_vec();
    message.extend_from_slice(value.split(|byte| *byte == 0).next().unwrap_or_default());
    message.extend_from_slice(
        b"\" for mail.cr_lf_mode. Must be one of: \"crlf\", \"lf\", \"mixed\", \"os\"",
    );
    message
}
