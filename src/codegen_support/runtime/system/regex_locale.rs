//! Purpose:
//! Documents why compiled regex calls no longer change the process `LC_CTYPE`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::{preg_match, preg_match_all, preg_replace,
//!   preg_split}` before compiling a pattern.
//!
//! Key details:
//! - php-src's pcre extension does not call `setlocale`; PCRE2 receives character
//!   tables for the current libc locale through `pcre2_maketables` in the native shim.
//! - Forcing `C.UTF-8` here would overwrite a user `setlocale(LC_CTYPE, ...)` and make
//!   `ctype_*` observe the wrong locale after any `preg_*` call.

use crate::codegen_support::emit::Emitter;

/// Keeps the previous call-site shape while relying on the PCRE2 shim for locale tables.
pub(crate) fn emit_prepare_regex_locale(_emitter: &mut Emitter) {}
