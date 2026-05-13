//! Purpose:
//! Provides the public parser entry points from spanned tokens to an AST program.
//! Coordinates statement parsing and optional recovery for collecting multiple syntax errors.
//!
//! Called from:
//! - `crate::pipeline::compile()` and `crate::resolver::files::parse_file()`.
//!
//! Key details:
//! - Parser output preserves spans and PHP syntax shape for later passes to rewrite safely.

pub mod ast;
mod attributes;
mod control;
pub mod expr;
mod stmt;

pub(crate) use attributes::{consume_attribute_lists, parse_attribute_lists};

pub use ast::Program;

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::span::Span;

pub fn parse(tokens: &[(Token, Span)]) -> Result<Program, CompileError> {
    match parse_with_recovery(tokens) {
        Ok(program) => Ok(program),
        Err(errors) => Err(CompileError::from_many(errors)),
    }
}

pub fn parse_with_recovery(tokens: &[(Token, Span)]) -> Result<Program, Vec<CompileError>> {
    let mut pos = 0;
    let mut stmts = Vec::new();
    let mut errors = Vec::new();

    // Skip OpenTag
    if pos < tokens.len() && tokens[pos].0 == Token::OpenTag {
        pos += 1;
    } else {
        let span = if pos < tokens.len() {
            tokens[pos].1
        } else {
            Span::dummy()
        };
        return Err(vec![CompileError::new(span, "Expected '<?php' open tag")]);
    }

    while pos < tokens.len() {
        if tokens[pos].0 == Token::Eof {
            break;
        }
        // Extern blocks can produce multiple stmts. Attributes on declarations
        // flow through parse_stmt below — extern is an elephc-specific block
        // that does not interact with PHP attributes.
        if tokens[pos].0 == Token::Extern {
            match stmt::parse_extern_stmts(tokens, &mut pos) {
                Ok(mut extern_stmts) => stmts.append(&mut extern_stmts),
                Err(error) => {
                    errors.extend(error.flatten());
                    stmt::recover_to_statement_boundary(tokens, &mut pos);
                }
            }
        } else {
            match stmt::parse_stmt(tokens, &mut pos) {
                Ok(stmt) => stmts.push(stmt),
                Err(error) => {
                    errors.extend(error.flatten());
                    stmt::recover_to_statement_boundary(tokens, &mut pos);
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(stmts)
    } else {
        Err(errors)
    }
}
