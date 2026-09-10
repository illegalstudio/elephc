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

use elephc_builtin_contract::mbstring_abi::ini::core::{lookup, Key, DIRECTIVES, REGISTRATION_ORDER};
use crate::{arrays::{ArrayGraph, Key as ArrayKey, Value}, coercion::Diagnostic};
use super::{IniString, ini::numeric};

/// One request's raw Core settings and their already-validated parser values.
#[derive(Clone)]
pub struct CoreIni {
    global: [IniString; DIRECTIVES.len()],
    local: [IniString; DIRECTIVES.len()],
    max_variables: i64,
    max_nesting: i64,
    display_errors: u8,
}

impl Default for CoreIni {
    /// Installs the catalog's non-null PHP defaults before any startup overrides.
    fn default() -> Self {
        let global = std::array::from_fn(|index| IniString::interned(DIRECTIVES[index].default.unwrap().as_bytes()));
        let max_variables = numeric::quantity(&global[Key::MaxVariables as usize]).0;
        let max_nesting = numeric::quantity(&global[Key::MaxNesting as usize]).0;
        let display_errors = numeric::display_errors(&global[Key::DisplayErrors as usize]);
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
            };
            if accepted {
                state.global[key as usize] = IniString::interned(value);
                state.local[key as usize] = state.global[key as usize].clone();
            }
        }
        (state, diagnostics)
    }

    /// Returns one independently retained raw local string, or None for an unknown exact name.
    pub fn get(&self, name: &[u8]) -> Option<IniString> { Some(self.local[lookup(name)? as usize].scalar_result()) }

    /// Applies a permitted runtime setting and returns its previous exact string identity.
    pub fn set(&mut self, name: &[u8], value: IniString) -> Option<IniString> {
        let key = lookup(name)?;
        if DIRECTIVES[key as usize].access & 1 == 0 { return None; }
        self.display_errors = numeric::display_errors(&value);
        Some(std::mem::replace(&mut self.local[key as usize], value).scalar_result())
    }

    /// Restores a user-modifiable setting to its startup string and parsed value.
    pub fn restore(&mut self, name: &[u8]) {
        if let Some(key) = lookup(name) { self.set(name, self.global[key as usize].clone()); }
    }

    /// Restores request-local mutations while retaining validated startup limits and separators.
    pub fn reset(&mut self) {
        self.local = self.global.clone();
        self.display_errors = numeric::display_errors(&self.local[Key::DisplayErrors as usize]);
    }

    /// Borrows the C-string separator bytes until the next request mutation or reset.
    pub fn separators(&self) -> &[u8] {
        self.local[Key::Separators as usize].split(|byte| *byte == 0).next().unwrap()
    }

    /// Returns the validated whole-query input variable limit, including zero.
    pub fn max_variables(&self) -> i64 { self.max_variables }

    /// Returns the validated nesting limit read before each field registration.
    pub fn max_nesting(&self) -> i64 { self.max_nesting }

    /// Returns PHP's disabled, stdout, or stderr display mode without collapsing its raw INI value.
    pub fn display_errors(&self) -> u8 { self.display_errors }

    /// Builds a sorted Core directive graph with an identity owner for every string-valued cell.
    pub fn all(&self, details: bool) -> (ArrayGraph, Vec<(usize, usize, IniString)>) {
        let mut arrays = vec![Vec::new()];
        let mut strings = Vec::new();
        for (index, directive) in DIRECTIVES.iter().enumerate() {
            let mut raw = |value: &IniString, array, entry| {
                strings.push((array, entry, value.clone()));
                Value::String(value.to_vec())
            };
            let value = if details {
                let child = arrays.len();
                arrays.push(vec![
                    (ArrayKey::String(b"global_value".to_vec()), raw(&self.global[index], child, 0)),
                    (ArrayKey::String(b"local_value".to_vec()), raw(&self.local[index], child, 1)),
                    (ArrayKey::String(b"access".to_vec()), Value::Int(directive.access as i64)),
                ]);
                Value::Array(child)
            } else { raw(&self.local[index], 0, index) };
            arrays[0].push((ArrayKey::String(directive.name.as_bytes().to_vec()), value));
        }
        (ArrayGraph::new(0, arrays).expect("unique Core keys and complete detail children"), strings)
    }
}
