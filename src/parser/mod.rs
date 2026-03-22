pub mod ast;
mod control;
pub mod expr;
mod stmt;

pub use ast::Program;

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::span::Span;

pub fn parse(tokens: &[(Token, Span)]) -> Result<Program, CompileError> {
    let mut pos = 0;
    let mut stmts = Vec::new();

    // Skip OpenTag
    if pos < tokens.len() && tokens[pos].0 == Token::OpenTag {
        pos += 1;
    } else {
        let span = if pos < tokens.len() {
            tokens[pos].1
        } else {
            Span::dummy()
        };
        return Err(CompileError::new(span, "Expected '<?php' open tag"));
    }

    while pos < tokens.len() {
        if tokens[pos].0 == Token::Eof {
            break;
        }
        let s = stmt::parse_stmt(tokens, &mut pos)?;
        stmts.push(s);
    }

    Ok(stmts)
}
