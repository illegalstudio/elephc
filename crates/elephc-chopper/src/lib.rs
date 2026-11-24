//! Purpose:
//! Provides PHP scanning, tokenization and parsing for Elephc and its runtime bridges.
//!
//! Called from:
//! - The compiler frontend, Magician and the PHP tokenizer C bridge.
//!
//! Key details:
//! - The scanner keeps original source bytes and exposes shared lexical rules.
//! - The syntax module builds compiler ASTs and validates TOKEN_PARSE requests.

pub mod pfa;
pub mod scanner;
pub mod syntax;
