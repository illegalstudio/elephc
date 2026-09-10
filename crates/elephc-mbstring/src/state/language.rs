//! Purpose:
//! Resolves mbstring language identities and their detection and mail defaults.
//!
//! Called from:
//! - Shared request state, encoding-list expansion, and mail operations.
//!
//! Key details:
//! - Canonical names and aliases are independent of operating-system locales.
//! - Language changes update auto expansion without replacing the active detection list.

use crate::encoding::Encoding;
use super::language_data::LANGUAGES;

/// Captured settings for one PHP language identity.
pub(super) struct LanguageInfo {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub detect_order: &'static [&'static str],
    pub mail_charset: &'static str,
    pub mail_header_encoding: &'static str,
    pub mail_body_encoding: &'static str,
}

/// One canonical language in the shared metadata catalog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Language(usize);

impl Language {
    /// Resolves case-insensitive canonical names and aliases up to the first NUL byte.
    pub fn lookup(name: &[u8]) -> Option<Self> {
        let name = name.split(|&byte| byte == 0).next().unwrap_or_default();
        LANGUAGES.iter().position(|entry| name.eq_ignore_ascii_case(entry.name.as_bytes())
            || entry.aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias.as_bytes()))).map(Self)
    }

    /// Returns the canonical language spelling used by PHP getters.
    pub fn name(self) -> &'static str { LANGUAGES[self.0].name }

    /// Expands the language's auto list using the authoritative encoding catalog.
    pub fn detect_order(self) -> Vec<Encoding> {
        LANGUAGES[self.0].detect_order.iter().map(|name| Encoding::lookup(name.as_bytes()).expect("language encoding")).collect()
    }

    /// Returns the mail charset, header transfer encoding, and body transfer encoding.
    pub fn mail_encodings(self) -> [Encoding; 3] {
        let entry = &LANGUAGES[self.0];
        [entry.mail_charset, entry.mail_header_encoding, entry.mail_body_encoding]
            .map(|name| Encoding::lookup(name.as_bytes()).expect("language mail encoding"))
    }
}
