//! Purpose:
//! Owns the compiler frontend and the parser mode used by TOKEN_PARSE.
//!
//! Called from:
//! - The compiler crate and the tokenizer runtime bridge.
//!
//! Key details:
//! - Compiler tokens, the AST and source profiles are shared by the frontend consumers.
//! - The tokenizer validates the PHP grammar using the same parser as the compiler.

pub mod call_args;
pub mod compiler_stack;
pub mod errors;
pub mod generics;
pub mod lexer;
pub mod names;
pub mod parser;
pub mod php_tokenizer;
pub mod source;
pub mod span;
pub mod string_bytes;
pub mod tokenizer_validation;
pub mod types;
