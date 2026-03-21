pub mod ast;
pub mod expr;
mod stmt;

pub use ast::Program;

use crate::errors::CompileError;
use crate::lexer::Token;

pub fn parse(tokens: &[Token]) -> Result<Program, CompileError> {
    let mut pos = 0;
    let mut stmts = Vec::new();

    // Skip OpenTag
    if pos < tokens.len() && tokens[pos] == Token::OpenTag {
        pos += 1;
    } else {
        return Err(CompileError::at(0, 0, "Expected '<?php' open tag"));
    }

    while pos < tokens.len() {
        if tokens[pos] == Token::Eof {
            break;
        }
        let s = stmt::parse_stmt(tokens, &mut pos)?;
        stmts.push(s);
    }

    Ok(stmts)
}
