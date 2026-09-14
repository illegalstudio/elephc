//! Purpose:
//! Defines the mbstring INI directives and their raw defaults and modification permissions.
//!
//! Called from:
//! - Compiler startup selection and shared request configuration, getters, and initialization.
//!
//! Key details:
//! - Storage order matches ini_get_all; startup handler order follows PHP registration order.

/// Stable internal indices into the request's eleven directive entries.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Key { Detect, Translation, Input, Output, Mimetypes, Internal, Language, Retry, Stack, Strict, Substitute }

/// PHP declaration metadata independent of the mutable request values.
pub struct Directive { pub name: &'static str, pub default: Option<&'static str>, pub access: u8 }

/// Authoritative directive names, raw defaults, and PHP modification masks.
pub const DIRECTIVES: [Directive; 11] = [
    Directive { name: "mbstring.detect_order", default: None, access: 7 },
    Directive { name: "mbstring.encoding_translation", default: Some("0"), access: 6 },
    Directive { name: "mbstring.http_input", default: None, access: 7 },
    Directive { name: "mbstring.http_output", default: None, access: 7 },
    Directive { name: "mbstring.http_output_conv_mimetypes", default: Some(r"^(text/|application/xhtml\+xml)"), access: 7 },
    Directive { name: "mbstring.internal_encoding", default: None, access: 7 },
    Directive { name: "mbstring.language", default: Some("neutral"), access: 7 },
    Directive { name: "mbstring.regex_retry_limit", default: Some("1000000"), access: 7 },
    Directive { name: "mbstring.regex_stack_limit", default: Some("100000"), access: 7 },
    Directive { name: "mbstring.strict_detection", default: Some("0"), access: 7 },
    Directive { name: "mbstring.substitute_character", default: None, access: 7 },
];

/// PHP registration order for handlers whose startup effects depend on earlier directives.
pub const REGISTRATION_ORDER: [Key; 11] = [Key::Language, Key::Detect, Key::Input, Key::Output,
    Key::Internal, Key::Substitute, Key::Translation, Key::Mimetypes, Key::Strict, Key::Stack, Key::Retry];

/// Resolves exact, case-sensitive directive bytes without truncating embedded NULs.
pub fn lookup(name: &[u8]) -> Option<Key> {
    REGISTRATION_ORDER.into_iter().find(|key| DIRECTIVES[*key as usize].name.as_bytes() == name)
}
