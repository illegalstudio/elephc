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

/// Stable indices in Core directive storage. Existing query keys retain their original values;
/// mail keys are appended so the native ABI never reinterprets an older index.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Key {
    Separators,
    DisplayErrors,
    MaxNesting,
    MaxVariables,
    Smtp,
    SmtpPort,
    SendmailFrom,
    SendmailPath,
    MailForceExtraParameters,
    MailMixedLfAndCrlf,
    MailCrLfMode,
}

#[cfg(windows)]
const DEFAULT_SENDMAIL_PATH: Option<&str> = None;
#[cfg(not(windows))]
const DEFAULT_SENDMAIL_PATH: Option<&str> = Some("/usr/sbin/sendmail -t -i");

/// Core declarations needed by the query parser and its diagnostic policy.
pub const DIRECTIVES: [Directive; 11] = [
    Directive { name: "arg_separator.input", default: Some("&"), access: 6 },
    Directive { name: "display_errors", default: Some("1"), access: 7 },
    Directive { name: "max_input_nesting_level", default: Some("64"), access: 6 },
    Directive { name: "max_input_vars", default: Some("1000"), access: 6 },
    Directive { name: "SMTP", default: Some("localhost"), access: 7 },
    Directive { name: "smtp_port", default: Some("25"), access: 7 },
    Directive { name: "sendmail_from", default: None, access: 7 },
    Directive { name: "sendmail_path", default: DEFAULT_SENDMAIL_PATH, access: 4 },
    Directive { name: "mail.force_extra_parameters", default: None, access: 6 },
    Directive { name: "mail.mixed_lf_and_crlf", default: Some("0"), access: 6 },
    Directive { name: "mail.cr_lf_mode", default: Some("crlf"), access: 6 },
];

/// Core startup handler order, before extension INI registration.
pub const REGISTRATION_ORDER: [Key; 11] = [
    Key::DisplayErrors,
    Key::Separators,
    Key::MaxNesting,
    Key::MaxVariables,
    Key::Smtp,
    Key::SmtpPort,
    Key::MailMixedLfAndCrlf,
    Key::MailCrLfMode,
    Key::SendmailFrom,
    Key::SendmailPath,
    Key::MailForceExtraParameters,
];

/// Name-sorted order used by `ini_get_all()` independently of stable storage indices.
pub const SORTED_KEYS: [Key; 11] = [
    Key::Smtp,
    Key::Separators,
    Key::DisplayErrors,
    Key::MailCrLfMode,
    Key::MailForceExtraParameters,
    Key::MailMixedLfAndCrlf,
    Key::MaxNesting,
    Key::MaxVariables,
    Key::SendmailFrom,
    Key::SendmailPath,
    Key::SmtpPort,
];

/// Looks up a supported Core directive without accepting case changes or embedded NUL suffixes.
pub fn lookup(name: &[u8]) -> Option<Key> {
    REGISTRATION_ORDER.into_iter().find(|key| DIRECTIVES[*key as usize].name.as_bytes() == name)
}
