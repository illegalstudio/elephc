//! Purpose:
//! Re-exports the shared PHP string-literal byte representation for the compiler frontend.
//!
//! Called from:
//! - Lexer escape decoding, optimizer byte operations, and codegen data emission.
//!
//! Key details:
//! - AOT and Magician use the same collision-safe encoding for non-UTF-8 escaped bytes.

pub(crate) use elephc_builtin_contract::string_literal::*;
