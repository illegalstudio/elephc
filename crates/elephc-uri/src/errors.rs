//! Purpose:
//! Shared error vocabulary for the URI bridge: validation error records and the stable
//! numeric type codes exposed to the PHP prelude.
//!
//! Called from:
//! - `crate::whatwg`, `crate::lib`, and the prelude's `UrlValidationError` mapping.
//!
//! Key details:
//! - Codes match the declaration order of `Uri\WhatWg\UrlValidationErrorType`; changing one
//!   is a PHP-visible break.
//! - Contexts are raw bytes because inputs are binary-safe; the C ABI hex-encodes them.

/// One WHATWG URL validation log entry.
#[derive(Clone, Debug)]
pub struct ValidationError {
    /// Original input bytes the entry points at, truncated at the first NUL like PHP's
    /// `zend_update_property_string` does.
    pub context: Vec<u8>,
    /// Stable `UrlValidationErrorType` code.
    pub type_code: i64,
    /// Whether this entry is fatal (`failure`) or a warning.
    pub failure: bool,
}

/// A failed operation's message and optional validation log.
#[derive(Clone, Debug)]
pub struct UriError {
    /// PHP-visible exception message.
    pub message: String,
    /// Validation entries in the order PHP exposes them.
    pub errors: Vec<ValidationError>,
}

/// `Uri\WhatWg\UrlValidationErrorType::DomainToAscii`.
pub const DOMAIN_TO_ASCII: i64 = 0;
/// `Uri\WhatWg\UrlValidationErrorType::DomainToUnicode`.
pub const DOMAIN_TO_UNICODE: i64 = 1;
/// `Uri\WhatWg\UrlValidationErrorType::DomainInvalidCodePoint`.
pub const DOMAIN_INVALID_CODE_POINT: i64 = 2;
/// `Uri\WhatWg\UrlValidationErrorType::HostInvalidCodePoint`.
pub const HOST_INVALID_CODE_POINT: i64 = 3;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4EmptyPart`.
pub const IPV4_EMPTY_PART: i64 = 4;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4TooManyParts`.
pub const IPV4_TOO_MANY_PARTS: i64 = 5;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4NonNumericPart`.
pub const IPV4_NON_NUMERIC_PART: i64 = 6;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4NonDecimalPart`.
pub const IPV4_NON_DECIMAL_PART: i64 = 7;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4OutOfRangePart`.
pub const IPV4_OUT_OF_RANGE_PART: i64 = 8;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv6Unclosed`.
pub const IPV6_UNCLOSED: i64 = 9;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv6InvalidCompression`.
pub const IPV6_INVALID_COMPRESSION: i64 = 10;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv6TooManyPieces`.
pub const IPV6_TOO_MANY_PIECES: i64 = 11;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv6MultipleCompression`.
pub const IPV6_MULTIPLE_COMPRESSION: i64 = 12;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv6InvalidCodePoint`.
pub const IPV6_INVALID_CODE_POINT: i64 = 13;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv6TooFewPieces`.
pub const IPV6_TOO_FEW_PIECES: i64 = 14;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4InIpv6TooManyPieces`.
pub const IPV4_IN_IPV6_TOO_MANY_PIECES: i64 = 15;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4InIpv6InvalidCodePoint`.
pub const IPV4_IN_IPV6_INVALID_CODE_POINT: i64 = 16;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4InIpv6OutOfRangePart`.
pub const IPV4_IN_IPV6_OUT_OF_RANGE_PART: i64 = 17;
/// `Uri\WhatWg\UrlValidationErrorType::Ipv4InIpv6TooFewParts`.
pub const IPV4_IN_IPV6_TOO_FEW_PARTS: i64 = 18;
/// `Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit`.
pub const INVALID_URL_UNIT: i64 = 19;
/// `Uri\WhatWg\UrlValidationErrorType::SpecialSchemeMissingFollowingSolidus`.
pub const SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS: i64 = 20;
/// `Uri\WhatWg\UrlValidationErrorType::MissingSchemeNonRelativeUrl`.
pub const MISSING_SCHEME_NON_RELATIVE_URL: i64 = 21;
/// `Uri\WhatWg\UrlValidationErrorType::InvalidReverseSoldius` (php-src's spelling).
pub const INVALID_REVERSE_SOLIDUS: i64 = 22;
/// `Uri\WhatWg\UrlValidationErrorType::InvalidCredentials`.
pub const INVALID_CREDENTIALS: i64 = 23;
/// `Uri\WhatWg\UrlValidationErrorType::HostMissing`.
pub const HOST_MISSING: i64 = 24;
/// `Uri\WhatWg\UrlValidationErrorType::PortOutOfRange`.
pub const PORT_OUT_OF_RANGE: i64 = 25;
/// `Uri\WhatWg\UrlValidationErrorType::PortInvalid`.
pub const PORT_INVALID: i64 = 26;
/// `Uri\WhatWg\UrlValidationErrorType::FileInvalidWindowsDriveLetter`.
pub const FILE_INVALID_WINDOWS_DRIVE_LETTER: i64 = 27;
/// `Uri\WhatWg\UrlValidationErrorType::FileInvalidWindowsDriveLetterHost`.
pub const FILE_INVALID_WINDOWS_DRIVE_LETTER_HOST: i64 = 28;

/// Returns the PHP enum case name for one numeric error type.
pub fn type_name(code: i64) -> &'static str {
    match code {
        DOMAIN_TO_ASCII => "DomainToAscii",
        DOMAIN_TO_UNICODE => "DomainToUnicode",
        DOMAIN_INVALID_CODE_POINT => "DomainInvalidCodePoint",
        HOST_INVALID_CODE_POINT => "HostInvalidCodePoint",
        IPV4_EMPTY_PART => "Ipv4EmptyPart",
        IPV4_TOO_MANY_PARTS => "Ipv4TooManyParts",
        IPV4_NON_NUMERIC_PART => "Ipv4NonNumericPart",
        IPV4_NON_DECIMAL_PART => "Ipv4NonDecimalPart",
        IPV4_OUT_OF_RANGE_PART => "Ipv4OutOfRangePart",
        IPV6_UNCLOSED => "Ipv6Unclosed",
        IPV6_INVALID_COMPRESSION => "Ipv6InvalidCompression",
        IPV6_TOO_MANY_PIECES => "Ipv6TooManyPieces",
        IPV6_MULTIPLE_COMPRESSION => "Ipv6MultipleCompression",
        IPV6_INVALID_CODE_POINT => "Ipv6InvalidCodePoint",
        IPV6_TOO_FEW_PIECES => "Ipv6TooFewPieces",
        IPV4_IN_IPV6_TOO_MANY_PIECES => "Ipv4InIpv6TooManyPieces",
        IPV4_IN_IPV6_INVALID_CODE_POINT => "Ipv4InIpv6InvalidCodePoint",
        IPV4_IN_IPV6_OUT_OF_RANGE_PART => "Ipv4InIpv6OutOfRangePart",
        IPV4_IN_IPV6_TOO_FEW_PARTS => "Ipv4InIpv6TooFewParts",
        INVALID_URL_UNIT => "InvalidUrlUnit",
        SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS => "SpecialSchemeMissingFollowingSolidus",
        MISSING_SCHEME_NON_RELATIVE_URL => "MissingSchemeNonRelativeUrl",
        INVALID_REVERSE_SOLIDUS => "InvalidReverseSoldius",
        INVALID_CREDENTIALS => "InvalidCredentials",
        HOST_MISSING => "HostMissing",
        PORT_OUT_OF_RANGE => "PortOutOfRange",
        PORT_INVALID => "PortInvalid",
        FILE_INVALID_WINDOWS_DRIVE_LETTER => "FileInvalidWindowsDriveLetter",
        FILE_INVALID_WINDOWS_DRIVE_LETTER_HOST => "FileInvalidWindowsDriveLetterHost",
        _ => "InvalidUrlUnit",
    }
}

/// Builds a validation entry, truncating its context at the first NUL.
pub fn entry(context: &[u8], type_code: i64, failure: bool) -> ValidationError {
    let end = context
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(context.len());
    ValidationError {
        context: context[..end].to_vec(),
        type_code,
        failure,
    }
}
