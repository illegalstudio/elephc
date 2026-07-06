//! Purpose:
//! Shared stream-filter helper emitters used by active EIR lowering.
//! Owns libz, bzip2, and iconv attachment helpers that publish per-program runtime hooks.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io` for EIR stream builtins.
//!
//! Key details:
//! - Helpers must keep optional native library references in user assembly so
//!   programs that do not use those filters do not link unused bridge symbols.

pub(crate) mod bzip2;
pub(crate) mod compress_bzip2_stream;
pub(crate) mod iconv;
pub(crate) mod iconv_write;
pub(crate) mod inflate;
pub(crate) mod zlib;
