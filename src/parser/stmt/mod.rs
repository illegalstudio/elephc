mod assign;
mod ffi;
mod namespace_use;
mod oop;
pub(crate) mod params;
mod simple;

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Stmt, StmtKind};
use crate::parser::control;
use crate::parser::expr::parse_expr;
use crate::span::Span;

pub use ffi::parse_extern_stmts;
pub(crate) use params::{looks_like_typed_param, parse_type_expr};

pub fn parse_stmt(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Stmt, CompileError> {
    let span = tokens[*pos].1;

    match &tokens[*pos].0 {
        Token::Echo | Token::Print => simple::parse_echo(tokens, pos, span),
        Token::Variable(_) => assign::parse_variable_stmt(tokens, pos, span),
        Token::This => simple::parse_this_stmt(tokens, pos, span),
        Token::PlusPlus | Token::MinusMinus => assign::parse_incdec_stmt(tokens, pos, span),
        Token::Class => oop::parse_class_decl(tokens, pos, span, false, false, false),
        Token::Enum => oop::parse_enum_decl(tokens, pos, span),
        Token::ReadOnly => oop::parse_readonly_decl(tokens, pos, span),
        Token::Packed => oop::parse_packed_decl(tokens, pos, span),
        Token::Interface => oop::parse_interface_decl(tokens, pos, span),
        Token::Trait => oop::parse_trait_decl(tokens, pos, span),
        Token::Abstract => oop::parse_abstract_decl(tokens, pos, span),
        Token::Final => oop::parse_final_decl(tokens, pos, span),
        Token::Function => params::parse_function_decl(tokens, pos, span),
        Token::Namespace => namespace_use::parse_namespace_stmt(tokens, pos, span),
        Token::Use => namespace_use::parse_use_stmt(tokens, pos, span),
        Token::Return => simple::parse_return(tokens, pos, span),
        Token::Throw => simple::parse_throw(tokens, pos, span),
        Token::Include => simple::parse_include(tokens, pos, span, false, false),
        Token::IncludeOnce => simple::parse_include(tokens, pos, span, true, false),
        Token::Require => simple::parse_include(tokens, pos, span, false, true),
        Token::RequireOnce => simple::parse_include(tokens, pos, span, true, true),
        Token::Const => simple::parse_const_decl(tokens, pos, span),
        Token::Global => assign::parse_global(tokens, pos, span),
        Token::Static => {
            if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::DoubleColon {
                let expr = parse_expr(tokens, pos)?;
                expect_semicolon(tokens, pos)?;
                Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
            } else {
                assign::parse_static_var(tokens, pos, span)
            }
        }
        Token::LBracket => assign::parse_list_unpack(tokens, pos, span),
        Token::Identifier(_)
        | Token::Self_
        | Token::Parent
        | Token::Backslash
        | Token::Question => {
            if assign::looks_like_typed_assign(tokens, *pos) {
                return assign::parse_typed_assign(tokens, pos, span);
            }
            let expr = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
        }
        // Control flow — delegated to control.rs
        Token::Switch => control::parse_switch(tokens, pos, span),
        Token::If => control::parse_if(tokens, pos, span),
        Token::IfDef => control::parse_ifdef(tokens, pos, span),
        Token::Try => control::parse_try(tokens, pos, span),
        Token::While => control::parse_while(tokens, pos, span),
        Token::Do => control::parse_do_while(tokens, pos, span),
        Token::For => control::parse_for(tokens, pos, span),
        Token::Foreach => control::parse_foreach(tokens, pos, span),
        Token::Break => {
            *pos += 1;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Break, span))
        }
        Token::Continue => {
            *pos += 1;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Continue, span))
        }
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token at statement position: {:?}", other),
        )),
    }
}

pub(crate) fn recover_to_statement_boundary(tokens: &[(Token, Span)], pos: &mut usize) {
    let start = *pos;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;

    while *pos < tokens.len() {
        match tokens[*pos].0 {
            Token::LParen => {
                paren_depth += 1;
                *pos += 1;
            }
            Token::RParen => {
                paren_depth = paren_depth.saturating_sub(1);
                *pos += 1;
            }
            Token::LBracket => {
                bracket_depth += 1;
                *pos += 1;
            }
            Token::RBracket => {
                bracket_depth = bracket_depth.saturating_sub(1);
                *pos += 1;
            }
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 => {
                *pos += 1;
                break;
            }
            Token::RBrace if paren_depth == 0 && bracket_depth == 0 => {
                break;
            }
            Token::Eof if paren_depth == 0 && bracket_depth == 0 => {
                break;
            }
            Token::Echo
            | Token::Print
            | Token::Variable(_)
            | Token::This
            | Token::PlusPlus
            | Token::MinusMinus
            | Token::Class
            | Token::Enum
            | Token::ReadOnly
            | Token::Packed
            | Token::Interface
            | Token::Trait
            | Token::Abstract
            | Token::Final
            | Token::Function
            | Token::Namespace
            | Token::Use
            | Token::Return
            | Token::Throw
            | Token::Include
            | Token::IncludeOnce
            | Token::Require
            | Token::RequireOnce
            | Token::Const
            | Token::Global
            | Token::Static
            | Token::Identifier(_)
            | Token::Self_
            | Token::Parent
            | Token::Backslash
            | Token::Question
            | Token::Switch
            | Token::If
            | Token::IfDef
            | Token::Try
            | Token::While
            | Token::Do
            | Token::For
            | Token::Foreach
            | Token::Break
            | Token::Continue
                if *pos > start && paren_depth == 0 && bracket_depth == 0 =>
            {
                break;
            }
            _ => {
                *pos += 1;
            }
        }
    }

    if *pos == start && *pos < tokens.len() && !matches!(tokens[*pos].0, Token::Eof) {
        *pos += 1;
    }
}

pub fn parse_block(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    let span = if *pos < tokens.len() {
        tokens[*pos].1
    } else {
        Span::dummy()
    };
    expect_token(tokens, pos, &Token::LBrace, "Expected '{'")?;

    let mut stmts = Vec::new();
    let mut errors = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        match parse_stmt(tokens, pos) {
            Ok(stmt) => stmts.push(stmt),
            Err(error) => {
                errors.extend(error.flatten());
                recover_to_statement_boundary(tokens, pos);
            }
        }
    }

    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
        errors.push(CompileError::new(span, "Expected '}'"));
        return Err(CompileError::from_many(errors));
    }
    *pos += 1;

    if errors.is_empty() {
        Ok(stmts)
    } else {
        Err(CompileError::from_many(errors))
    }
}

/// Parse either a braced block `{ ... }` or a single statement (for braceless if/while/for/foreach).
pub fn parse_body(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        parse_block(tokens, pos)
    } else {
        let stmt = parse_stmt(tokens, pos)?;
        Ok(vec![stmt])
    }
}

pub(crate) fn expect_semicolon(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, "Expected ';'"))
    }
}

pub(crate) fn expect_token(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    expected: &Token,
    msg: &str,
) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == *expected {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, msg))
    }
}

pub(crate) fn name_starts_at(tokens: &[(Token, Span)], pos: usize) -> bool {
    matches!(
        tokens.get(pos).map(|(t, _)| t),
        Some(Token::Identifier(_)) | Some(Token::Backslash)
    )
}

pub(crate) fn parse_name(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<Name, CompileError> {
    let mut kind = NameKind::Unqualified;
    if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
    }

    let mut parts = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                parts.push(name.clone());
                *pos += 1;
            }
            _ if parts.is_empty() => return Err(CompileError::new(span, first_error)),
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected identifier after '\\' in qualified name",
                ))
            }
        }

        if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
            if kind != NameKind::FullyQualified {
                kind = NameKind::Qualified;
            }
            *pos += 1;
            continue;
        }
        break;
    }

    Ok(Name::from_parts(kind, parts))
}
