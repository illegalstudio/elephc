//! Purpose:
//! Resolves the `$encoding` argument of PHP's case-mapping builtins the way mbstring's
//! `php_mb_get_encoding()` does, and picks the codec elephc runs for it.
//!
//! Called from:
//! - `crate::case::convert_case()`.
//!
//! Key details:
//! - Validity follows mbstring's encoding list, not libc iconv's: `tables::NAMES` holds
//!   every spelling `mbfl_name2encoding()` accepts, case-insensitively, including the name
//!   prefixes its perfect hash admits.
//! - php-src looks the name up as a C string, so everything from the first NUL byte on
//!   is ignored, both for the lookup and in the `ValueError` message.
//! - A handful of valid mbstring encodings have no codec here (the deprecated
//!   Base64/QPrint/HTML-entities/UUencode pseudo-encodings, the carrier emoji variants,
//!   the stateful UTF-7/HZ/ISO-2022 family, and EUC-TW); those are reported as
//!   unsupported rather than silently mapped through a charset that answers differently.

use super::codec::{ByteOrder, Codec};
use super::tables::{single_byte_table, NAMES};

/// What an `$encoding` argument selects.
pub(super) enum Resolved {
    /// A codec elephc implements; `turkish` is set for ISO-8859-9.
    Supported {
        /// The decoder/encoder pair.
        codec: Codec,
        /// Whether mbstring's ISO-8859-9 dotted/dotless `i` rules apply.
        turkish: bool,
    },
    /// A name mbstring accepts but elephc has no codec for.
    Unsupported,
    /// A name mbstring rejects.
    Invalid,
}

/// Truncates an encoding argument at its first NUL byte, as php-src's C-string lookup does.
pub(super) fn c_string_prefix(name: &[u8]) -> &[u8] {
    match name.iter().position(|&byte| byte == 0) {
        Some(nul) => &name[..nul],
        None => name,
    }
}

/// Resolves one explicit encoding name.
pub(super) fn resolve(name: &[u8]) -> Resolved {
    let lowered = c_string_prefix(name).to_ascii_lowercase();
    let Ok(index) = NAMES.binary_search_by(|&(known, _)| known.as_bytes().cmp(&lowered)) else {
        return Resolved::Invalid;
    };
    let canonical = NAMES[index].1;
    match codec_for(canonical) {
        Some(codec) => Resolved::Supported {
            codec,
            turkish: canonical == "ISO-8859-9",
        },
        None => Resolved::Unsupported,
    }
}

/// Returns the codec for one canonical mbstring encoding name.
fn codec_for(canonical: &str) -> Option<Codec> {
    if let Some(table) = single_byte_table(canonical) {
        return Some(Codec::SingleByte(table));
    }
    Some(match canonical {
        "UTF-8" => Codec::Utf8,
        "UTF-16" => Codec::Utf16(ByteOrder::Detect),
        "UTF-16BE" => Codec::Utf16(ByteOrder::Big),
        "UTF-16LE" => Codec::Utf16(ByteOrder::Little),
        "UTF-32" => Codec::Utf32(ByteOrder::Detect),
        "UTF-32BE" => Codec::Utf32(ByteOrder::Big),
        "UTF-32LE" => Codec::Utf32(ByteOrder::Little),
        "UCS-2" => Codec::Ucs2(ByteOrder::Detect),
        "UCS-2BE" => Codec::Ucs2(ByteOrder::Big),
        "UCS-2LE" => Codec::Ucs2(ByteOrder::Little),
        "UCS-4" => Codec::Ucs4(ByteOrder::Detect),
        "UCS-4BE" => Codec::Ucs4(ByteOrder::Big),
        "UCS-4LE" => Codec::Ucs4(ByteOrder::Little),
        // Stateless multi-byte encodings whose libc iconv tables reproduce mbstring's
        // results on valid text (checked differentially against php 8.5). The stateful
        // ones (UTF-7, HZ, ISO-2022-*) and EUC-TW, whose tables differ, stay unsupported.
        "EUC-JP" => Codec::Iconv("EUC-JP"),
        "SJIS" => Codec::Iconv("SHIFT_JIS"),
        "eucJP-win" => Codec::Iconv("EUC-JP-MS"),
        "EUC-JP-2004" => Codec::Iconv("EUC-JISX0213"),
        "SJIS-2004" => Codec::Iconv("SHIFT_JISX0213"),
        "CP932" | "SJIS-win" => Codec::Iconv("CP932"),
        "GB18030" | "GB18030-2022" => Codec::Iconv("GB18030"),
        "EUC-CN" => Codec::Iconv("EUC-CN"),
        "CP936" => Codec::Iconv("CP936"),
        "BIG-5" => Codec::Iconv("BIG5"),
        "CP950" => Codec::Iconv("CP950"),
        "EUC-KR" => Codec::Iconv("EUC-KR"),
        "UHC" => Codec::Iconv("CP949"),
        _ => return None,
    })
}
