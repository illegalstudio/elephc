//! Purpose:
//! Records PHP language names, aliases, default detection order, and mail encodings.
//!
//! Called from:
//! - `super::language` for shared request settings and mail defaults.
//!
//! Key details:
//! - Regenerate with `python3 scripts/mbstring/generate_languages.py`.
//! - Values come from the PHP language oracle, independently of platform locale.

use super::language::LanguageInfo;

/// Complete language catalog, with the default neutral language first.
pub(super) const LANGUAGES: &[LanguageInfo] = &[
    LanguageInfo {
        name: "neutral",
        aliases: &[],
        detect_order: &["ASCII", "UTF-8"],
        mail_charset: "UTF-8",
        mail_header_encoding: "BASE64",
        mail_body_encoding: "BASE64",
    },
    LanguageInfo {
        name: "uni",
        aliases: &["universal"],
        detect_order: &["ASCII", "UTF-8"],
        mail_charset: "UTF-8",
        mail_header_encoding: "BASE64",
        mail_body_encoding: "BASE64",
    },
    LanguageInfo {
        name: "English",
        aliases: &["en"],
        detect_order: &["ASCII", "UTF-8"],
        mail_charset: "ISO-8859-1",
        mail_header_encoding: "Quoted-Printable",
        mail_body_encoding: "8bit",
    },
    LanguageInfo {
        name: "German",
        aliases: &["de", "Deutsch"],
        detect_order: &["ASCII", "UTF-8"],
        mail_charset: "ISO-8859-15",
        mail_header_encoding: "Quoted-Printable",
        mail_body_encoding: "8bit",
    },
    LanguageInfo {
        name: "Japanese",
        aliases: &["ja"],
        detect_order: &["ASCII", "JIS", "UTF-8", "EUC-JP", "SJIS"],
        mail_charset: "ISO-2022-JP",
        mail_header_encoding: "BASE64",
        mail_body_encoding: "7bit",
    },
    LanguageInfo {
        name: "Korean",
        aliases: &["ko"],
        detect_order: &["ASCII", "UTF-8", "EUC-KR", "UHC"],
        mail_charset: "ISO-2022-KR",
        mail_header_encoding: "BASE64",
        mail_body_encoding: "7bit",
    },
    LanguageInfo {
        name: "Simplified Chinese",
        aliases: &["zh-cn"],
        detect_order: &["ASCII", "UTF-8", "EUC-CN", "CP936"],
        mail_charset: "HZ",
        mail_header_encoding: "BASE64",
        mail_body_encoding: "7bit",
    },
    LanguageInfo {
        name: "Traditional Chinese",
        aliases: &["zh-tw"],
        detect_order: &["ASCII", "UTF-8", "EUC-TW", "BIG-5"],
        mail_charset: "BIG-5",
        mail_header_encoding: "BASE64",
        mail_body_encoding: "8bit",
    },
    LanguageInfo {
        name: "Russian",
        aliases: &["ru"],
        detect_order: &["ASCII", "UTF-8", "KOI8-R", "Windows-1251", "CP866"],
        mail_charset: "KOI8-R",
        mail_header_encoding: "Quoted-Printable",
        mail_body_encoding: "8bit",
    },
    LanguageInfo {
        name: "Ukrainian",
        aliases: &["ua"],
        detect_order: &["ASCII", "UTF-8", "KOI8-U"],
        mail_charset: "KOI8-U",
        mail_header_encoding: "Quoted-Printable",
        mail_body_encoding: "8bit",
    },
    LanguageInfo {
        name: "Armenian",
        aliases: &["hy"],
        detect_order: &["ASCII", "UTF-8", "ArmSCII-8"],
        mail_charset: "ArmSCII-8",
        mail_header_encoding: "Quoted-Printable",
        mail_body_encoding: "8bit",
    },
    LanguageInfo {
        name: "Turkish",
        aliases: &["tr"],
        detect_order: &["ASCII", "UTF-8", "Windows-1254", "ISO-8859-9"],
        mail_charset: "ISO-8859-9",
        mail_header_encoding: "Quoted-Printable",
        mail_body_encoding: "8bit",
    },
];
