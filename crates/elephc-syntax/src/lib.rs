//! Purpose:
//! Owns elephc's PHP lexer, parser, AST, source profile and diagnostic value types.
//!
//! Called from:
//! - The compiler crate and the tokenizer runtime bridge.
//!
//! Key details:
//! - This crate has no runtime or compiler-backend dependency, so bridges link only the frontend.
//! - The parser and lexer keep their historical `crate::` module paths.

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
