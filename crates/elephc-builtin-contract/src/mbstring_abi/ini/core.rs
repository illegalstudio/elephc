//! Purpose:
//! Declares the Core INI settings consumed by mbstring query parsing.
//!
//! Called from:
//! - Compiler startup selection and the shared Core request-state provider.
//!
//! Key details:
//! - Raw defaults and access masks follow PHP 8.5; names use exact byte matching.
//! - These are Core directives, independent of the mbstring extension's own catalog.

use super::catalog::Directive;

/// Stable indices in sorted Core directive storage.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Key { Separators, DisplayErrors, MaxNesting, MaxVariables }

/// Core declarations needed by the query parser and its diagnostic policy.
pub const DIRECTIVES: [Directive; 4] = [
    Directive { name: "arg_separator.input", default: Some("&"), access: 6 },
    Directive { name: "display_errors", default: Some("1"), access: 7 },
    Directive { name: "max_input_nesting_level", default: Some("64"), access: 6 },
    Directive { name: "max_input_vars", default: Some("1000"), access: 6 },
];

/// Core startup handler order, before extension INI registration.
pub const REGISTRATION_ORDER: [Key; 4] = [Key::DisplayErrors, Key::Separators, Key::MaxNesting, Key::MaxVariables];

/// Looks up a supported Core directive without accepting case changes or embedded NUL suffixes.
pub fn lookup(name: &[u8]) -> Option<Key> {
    REGISTRATION_ORDER.into_iter().find(|key| DIRECTIVES[*key as usize].name.as_bytes() == name)
}
