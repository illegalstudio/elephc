//! Purpose:
//! Groups the RFC 2047 MIME header surface of PHP's iconv extension.
//!
//! Called from:
//! - `crate::abi` and Magician's `iconv_mime_*` eval bindings through `crate`'s re-exports.
//!
//! Key details:
//! - `base64` and `quoted_printable` are the two transfer encodings encoded-words use.
//! - `decode` ports php-src's encoded-word scanner state machine one state at a time.

pub mod base64;
pub mod decode;
pub mod encode;
pub mod quoted_printable;
