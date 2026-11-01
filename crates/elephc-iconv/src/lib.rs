//! Purpose:
//! Implements PHP's `iconv` extension once, for both elephc backends: the AOT runtime
//! reaches it through the `elephc_iconv_*` C ABI and Magician links the same Rust API.
//! It also hosts mbstring's `mb_strtoupper()` / `mb_strtolower()` (`crate::case`): those
//! need a charset layer for every non-UTF-8 encoding, and living here lets both backends
//! share one implementation and one bridge instead of growing a second one.
//!
//! Called from:
//! - `crate::abi`, which the compiled program's `__rt_iconv_*` helpers call through
//!   published function-pointer slots.
//! - `elephc-magician`'s `iconv*` eval bindings, directly as an rlib.
//!
//! Key details:
//! - Character conversion is delegated to the platform `iconv`, so charset coverage,
//!   `//TRANSLIT`, and `//IGNORE` behave exactly as they do under php-src.
//! - Every operation returns `IconvError`, which carries the severity and message text
//!   php-src emits, so both backends render identical diagnostics.
//! - The encoding trio set by `iconv_set_encoding()` is process-wide state owned here.
//! - Case mapping is not delegated to libc: it ports mbstring's own rules and tables so
//!   the result matches php byte for byte, and only a few CJK encodings go through iconv.

pub mod abi;
mod case;
mod convert;
mod encoding_state;
mod error;
mod ffi;
mod mime;
mod search;
mod text;

#[cfg(test)]
mod tests;

pub use case::{convert_case, CaseError, CaseMode};
pub use convert::convert;
pub use encoding_state::{effective_charset, get, set, EncodingKind, DEFAULT_ENCODING};
pub use error::{IconvError, IconvResult, Severity};
pub use ffi::charsets_are_convertible;
pub use mime::decode::{mime_decode, mime_decode_headers, MODE_CONTINUE_ON_ERROR, MODE_STRICT};
pub use mime::encode::{mime_encode, MimeEncodeOptions, Scheme, DEFAULT_LINE_BREAK, DEFAULT_LINE_LENGTH};
pub use search::{offset_value_error_message, strlen, strpos, strrpos, substr, SearchFailure};

/// Implementation identity elephc reports through the `ICONV_IMPL` constant.
///
/// php-src bakes this at build time from the libc it linked. elephc compiles ahead of
/// time for a target whose libc build is not knowable, so the value is derived from the
/// target platform: Apple platforms ship GNU libiconv, and elephc's Linux support
/// targets glibc.
pub const fn implementation_name(macos_target: bool) -> &'static str {
    if macos_target {
        "libiconv"
    } else {
        "glibc"
    }
}

/// Version elephc reports through the `ICONV_VERSION` constant.
///
/// The runtime libc version cannot be known while cross-compiling, so elephc reports
/// `unknown`; a normal php-src build instead records the provider version it detected.
pub const ICONV_VERSION: &str = "unknown";
