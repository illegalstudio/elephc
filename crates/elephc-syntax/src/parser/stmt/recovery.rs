//! Purpose:
//! Recovers the statement parser after a syntax error without consuming the next statement.
//!
//! Called from:
//! - `crate::parser::stmt::blocks` while accumulating block diagnostics.
//!
//! Key details:
//! - Parenthesis and bracket nesting suppress boundary detection until depth returns to zero.

use crate::lexer::{SpannedToken, Token};

/// Advances `pos` to the next PHP statement boundary following a parse error.
pub fn recover_to_statement_boundary(tokens: &[SpannedToken], pos: &mut usize) {
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
            Token::RBrace
            | Token::EndDeclare
            | Token::EndIf
            | Token::EndWhile
            | Token::EndFor
            | Token::EndForeach
            | Token::EndSwitch
            | Token::Eof
                if paren_depth == 0 && bracket_depth == 0 =>
            {
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
            | Token::Declare
            | Token::Return
            | Token::Throw
            | Token::Include
            | Token::IncludeOnce
            | Token::Require
            | Token::RequireOnce
            | Token::Const
            | Token::Global
            | Token::Static
            | Token::Goto
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
