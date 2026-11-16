//! Purpose:
//! Owns elephc's PHP lexer, parser, AST, source profile and diagnostic value types.
//!
//! Called from:
//! - The compiler crate and the tokenizer runtime bridge.
//!
//! Key details:
//! - This crate has no runtime or compiler-backend dependency, so bridges link only the frontend.
//! - The parser and lexer keep their historical `crate::` module paths.

pub mod call_args;
pub mod compiler_stack;
pub mod errors;
pub mod generics;
pub mod lexer;
pub mod names;
pub mod parser;
pub mod source;
pub mod span;
pub mod string_bytes;
pub mod tokenizer_validation;
pub mod types;

/// Rejects the validation-only feature when this source is compiled for the compiler parser.
///
/// The feature intentionally discards AST construction; enabling it through
/// `--all-features` on the workspace would silently break the compiler's parser. Only the
/// `elephc-syntax-validate` wrapper package may compile this source in that mode.
#[cfg(feature = "tokenizer-validation")]
const _: () = {
    assert!(
        env!("CARGO_PKG_NAME").len() == "elephc-syntax-validate".len(),
        "the tokenizer-validation feature is only supported through elephc-syntax-validate"
    );
};
