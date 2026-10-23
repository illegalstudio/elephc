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
pub enum Key { Separators, DisplayErrors, MaxNesting, MaxVariables, OpenBaseDir }

/// Core declarations needed by the query parser and its diagnostic policy.
pub const DIRECTIVES: [Directive; 5] = [
    Directive { name: "arg_separator.input", default: Some("&"), access: 6 },
    Directive { name: "display_errors", default: Some("1"), access: 7 },
    Directive { name: "max_input_nesting_level", default: Some("64"), access: 6 },
    Directive { name: "max_input_vars", default: Some("1000"), access: 6 },
    // PHP_INI_ALL: php-src's ext/curl checks read it through PG(open_basedir) and the
    // fixture `ini_set("open_basedir", …)` changes it at run time. The catalog default is
    // PHP's empty value (no restriction).
    Directive { name: "open_basedir", default: Some(""), access: 7 },
];

/// Core startup handler order, before extension INI registration. `open_basedir` parses
/// no quantity, so its position is cosmetic; it is last like in php-src's registration.
pub const REGISTRATION_ORDER: [Key; 5] =
    [Key::DisplayErrors, Key::Separators, Key::MaxNesting, Key::MaxVariables, Key::OpenBaseDir];

/// Parses the Core `display_errors` directive into php's display mode.
///
/// `0` disables the display copy, `1` renders it through the output layer (the CLI
/// default), `2` sends it to stderr. Textual modes and php's unsigned-byte C integer
/// conversion are both honoured, and an unparsable value keeps the enabled mode.
pub fn display_errors_mode(input: &[u8]) -> u8 {
    let names = [b"on".as_slice(), b"yes", b"true", b"stdout"];
    if names.iter().any(|name| input.eq_ignore_ascii_case(name)) {
        return 1;
    }
    if input.eq_ignore_ascii_case(b"stderr") {
        return 2;
    }
    let digits: Vec<u8> = input
        .iter()
        .copied()
        .skip_while(|byte| byte.is_ascii_whitespace())
        .take_while(|byte| byte.is_ascii_digit())
        .collect();
    let mode = digits
        .iter()
        .fold(0u8, |value, digit| value.wrapping_mul(10).wrapping_add(digit - b'0'));
    if mode > 2 { 1 } else { mode }
}

/// Looks up a supported Core directive without accepting case changes or embedded NUL suffixes.
pub fn lookup(name: &[u8]) -> Option<Key> {
    REGISTRATION_ORDER.into_iter().find(|key| DIRECTIVES[*key as usize].name.as_bytes() == name)
}
