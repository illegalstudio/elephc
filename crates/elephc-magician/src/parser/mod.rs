//! Purpose:
//! Parses runtime PHP eval fragments into EvalIR statement form.
//! The module entry point validates fragment boundaries, delegates tokenization,
//! and hands tokens to focused parser state.
//!
//! Called from:
//! - `crate::ffi::execute::__elephc_eval_execute()`
//! - `crate::interpreter` tests and nested eval execution paths.
//!
//! Key details:
//! - PHP eval fragments are statement fragments and must not include opening
//!   `<?` / `<?php` tags.
//! - File and directory metadata are supplied by the eval context at execution time.

mod cursor;
mod expressions;
mod state;
mod statements;

#[cfg(test)]
mod tests;

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::lexer::tokenize;
use state::Parser;

/// Parses an eval fragment into by-name EvalIR statements.
pub fn parse_fragment(code: &[u8]) -> Result<EvalProgram, EvalParseError> {
    if contains_php_open_tag(code) {
        return Err(EvalParseError::PhpOpenTag);
    }
    let source = std::str::from_utf8(code).map_err(|_| EvalParseError::InvalidUtf8)?;
    let tokens = tokenize(source)?;
    Parser::new(tokens, code.len()).parse_program()
}

/// Returns true when a fragment contains a PHP opening tag sequence.
fn contains_php_open_tag(code: &[u8]) -> bool {
    code.windows(2).any(|window| window == b"<?")
}
