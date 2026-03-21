mod cursor;
mod scan;
pub mod token;

pub use token::Token;

use crate::errors::CompileError;
use crate::span::Span;

pub fn tokenize(source: &str) -> Result<Vec<(Token, Span)>, CompileError> {
    scan::scan_tokens(source)
}
