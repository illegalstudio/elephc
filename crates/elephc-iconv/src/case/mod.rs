//! Purpose:
//! Implements mbstring's `mb_strtoupper()` and `mb_strtolower()` once, for both elephc
//! backends, next to the charset machinery their non-UTF-8 encodings need.
//!
//! Called from:
//! - `crate::abi::dispatch` for the AOT runtime's `OP_MB_STRTOUPPER` / `OP_MB_STRTOLOWER`.
//! - `elephc-magician`'s `mb_strtoupper` / `mb_strtolower` eval bindings, directly.
//!
//! Key details:
//! - The result is byte-for-byte php 8.5's: full Unicode case mapping, the final-sigma
//!   rule, one `?` per maximal invalid UTF-8 subpart, and mbstring's encoding names.
//! - An omitted or `null` encoding means UTF-8, mbstring's default internal encoding;
//!   elephc has no `mb_internal_encoding()` to change it.
//! - A rejected name becomes the exact php `ValueError` message; the message is returned
//!   as bytes because it quotes the caller's name verbatim, invalid UTF-8 included.

mod codec;
mod encoding;
mod mapping;
mod props;
mod tables;

#[cfg(test)]
mod tests;

/// Which of mbstring's case conversions to apply.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaseMode {
    /// `mb_strtoupper()` (`MB_CASE_UPPER`).
    Upper,
    /// `mb_strtolower()` (`MB_CASE_LOWER`).
    Lower,
}

impl CaseMode {
    /// Returns the PHP function that performs this conversion.
    pub fn function_name(self) -> &'static str {
        match self {
            CaseMode::Upper => "mb_strtoupper",
            CaseMode::Lower => "mb_strtolower",
        }
    }
}

/// Why a case conversion refused its `$encoding` argument.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CaseError {
    /// mbstring does not know the name (the name as php quotes it, cut at the first NUL).
    InvalidEncoding(Vec<u8>),
    /// mbstring knows the name, but elephc implements no codec for it.
    UnsupportedEncoding(Vec<u8>),
}

impl CaseError {
    /// Renders the `ValueError` message the builtin throws.
    ///
    /// An invalid name reproduces php-src's `zend_argument_value_error()` text exactly; an
    /// unsupported one keeps the same shape but says why elephc refuses a name php accepts.
    pub fn value_error_message(&self, function: &str) -> Vec<u8> {
        let (requirement, name) = match self {
            CaseError::InvalidEncoding(name) => ("must be a valid encoding", name),
            CaseError::UnsupportedEncoding(name) => ("must be an encoding elephc supports", name),
        };
        let mut message =
            format!("{function}(): Argument #2 ($encoding) {requirement}, \"").into_bytes();
        message.extend_from_slice(name);
        message.extend_from_slice(b"\" given");
        message
    }
}

/// Applies `mb_strtoupper()` / `mb_strtolower()` to `input` in the requested encoding.
///
/// `encoding` is `None` for an omitted or `null` argument, which selects UTF-8.
pub fn convert_case(
    mode: CaseMode,
    input: &[u8],
    encoding: Option<&[u8]>,
) -> Result<Vec<u8>, CaseError> {
    let (codec, turkish) = match encoding {
        None => (codec::Codec::Utf8, false),
        Some(name) => match encoding::resolve(name) {
            encoding::Resolved::Supported { codec, turkish } => (codec, turkish),
            encoding::Resolved::Unsupported => {
                return Err(CaseError::UnsupportedEncoding(
                    encoding::c_string_prefix(name).to_vec(),
                ));
            }
            encoding::Resolved::Invalid => {
                return Err(CaseError::InvalidEncoding(encoding::c_string_prefix(name).to_vec()));
            }
        },
    };
    mapping::convert(mode, codec, turkish, input).map_err(|_| {
        // Only a libc iconv fallback can fail, when the platform lacks that charset.
        CaseError::UnsupportedEncoding(encoding.map(encoding::c_string_prefix).unwrap_or_default().to_vec())
    })
}
