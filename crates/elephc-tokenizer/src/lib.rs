//! Purpose:
//! Exports the standalone tokenizer bridge without embedding its C symbols in Magician.
//!
//! Called from:
//! - Compiled PHP through the tokenizer prelude's extern declarations.
//!
//! Key details:
//! - Magician depends directly on the shared engine, avoiding duplicate C ABI symbols.
//! - The bridge owns snapshots while the engine owns lexical and parse semantics.

pub mod abi;
pub use elephc_tokenizer_engine::{
    token_name, tokenize, tokenize_with_warnings, ParseError, Token, TokenizeOutput, Warning, TOKEN_PARSE,
};
