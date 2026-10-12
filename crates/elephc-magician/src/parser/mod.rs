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
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod repl;
mod state;
mod statements;

#[cfg(test)]
mod tests;

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::lexer::tokenize;
use state::Parser;

/// Parses an eval fragment into by-name EvalIR statements.
///
/// A fragment used to be refused outright when it contained `<?` ANYWHERE, a string
/// included: `eval('echo "<?xml";')` failed where reference PHP 8.5.10 prints `<?xml`. The
/// lexer now owns the tags: after a `?>` it reads inline HTML up to the next `<?php`, and a
/// `<?php` met in code lexes as `<` then `?`, which the grammar refuses as PHP does
/// (`eval('<?php echo 1;')` is a parse error there too).
pub fn parse_fragment(code: &[u8]) -> Result<EvalProgram, EvalParseError> {
    let source = std::str::from_utf8(code).map_err(|_| EvalParseError::InvalidUtf8)?;
    let tokens = tokenize(source)?;
    Parser::new(tokens, code.len()).parse_program()
}

/// Parses a token stream assembled by the caller, ending in an EOF token.
///
/// An included file's stream is built from several blocks and the inline HTML between them
/// (`crate::script_cache::segments::parse_script`), and has to reach the grammar as one
/// program so that a block may span the tags. `source_len` is the whole file's length.
pub(crate) fn parse_tokens(
    tokens: Vec<crate::lexer::Token>,
    source_len: usize,
) -> Result<EvalProgram, EvalParseError> {
    Parser::new(tokens, source_len).parse_program()
}
