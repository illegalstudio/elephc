mod cursor;
mod scan;
pub mod token;

pub use token::Token;

use crate::errors::CompileError;

pub fn tokenize(source: &str) -> Result<Vec<Token>, CompileError> {
    scan::scan_tokens(source)
}
