//! Purpose:
//! Finds complete leading declare statements without creating or stamping AST nodes.
//!
//! Called from:
//! - `super::declare` for strict-types preseeding and directive placement checks.
//!
//! Key details:
//! - A control statement includes its else/catch/finally/do-while tails and ifdef blocks.
//! - Alternative syntax nests through the same statement scanner; ordinary bodies stay opaque.

use crate::lexer::{SpannedToken, Token};

/// Checks that the preceding prefix consists of complete declares and empty statements.
pub(super) fn only_declares_and_empty_statements_before(
    tokens: &[SpannedToken],
    declare_pos: usize,
) -> bool {
    let mut pos = 1;
    while pos < declare_pos {
        match &tokens[pos].0 {
            Token::Semicolon => pos += 1,
            Token::Declare => match skip_declare_statement(tokens, pos) {
                Some(next) if next <= declare_pos => pos = next,
                _ => return false,
            },
            _ => return false,
        }
    }
    pos == declare_pos
}

/// Returns the token after one complete declare, including its entire single-statement body.
pub(super) fn skip_declare_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let body = skip_group(tokens, start + 1, &Token::LParen, &Token::RParen)?;
    if tokens.get(body)?.0 == Token::Colon {
        skip_alternative_block(tokens, body, &Token::EndDeclare)
    } else {
        skip_single_statement(tokens, body)
    }
}

/// Matches a balanced group; strings and comments have already been tokenized.
fn skip_group(
    tokens: &[SpannedToken],
    start: usize,
    open: &Token,
    close: &Token,
) -> Option<usize> {
    if &tokens.get(start)?.0 != open {
        return None;
    }
    let mut depth = 0usize;
    for (pos, (token, _)) in tokens.iter().enumerate().skip(start) {
        if token == open {
            depth += 1;
        } else if token == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(pos + 1);
            }
        } else if token == &Token::Eof {
            return None;
        }
    }
    None
}

/// Consumes a required statement terminator at the supplied position.
fn skip_semicolon(tokens: &[SpannedToken], pos: usize) -> Option<usize> {
    (tokens.get(pos)?.0 == Token::Semicolon).then_some(pos + 1)
}

/// Scans one statement, keeping every control-flow tail attached to its opening statement.
fn skip_single_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    match &tokens.get(start)?.0 {
        Token::AttrOpen => {
            let body = skip_attribute_groups(tokens, start)?;
            skip_single_statement(tokens, body)
        }
        Token::Semicolon => Some(start + 1),
        Token::LBrace => skip_group(tokens, start, &Token::LBrace, &Token::RBrace),
        Token::Declare => skip_declare_statement(tokens, start),
        Token::If => skip_if_statement(tokens, start),
        Token::IfDef => {
            if !matches!(&tokens.get(start + 1)?.0, Token::Identifier(_)) {
                return None;
            }
            let mut end = skip_group(tokens, start + 2, &Token::LBrace, &Token::RBrace)?;
            if tokens.get(end)?.0 == Token::Else {
                end = skip_group(tokens, end + 1, &Token::LBrace, &Token::RBrace)?;
            }
            Some(end)
        }
        Token::Do => {
            let tail = skip_single_statement(tokens, start + 1)?;
            if tokens.get(tail)?.0 != Token::While {
                return None;
            }
            let end = skip_group(tokens, tail + 1, &Token::LParen, &Token::RParen)?;
            skip_semicolon(tokens, end)
        }
        Token::Try => skip_try_statement(tokens, start),
        Token::While | Token::For | Token::Foreach | Token::Switch => {
            let body = skip_group(tokens, start + 1, &Token::LParen, &Token::RParen)?;
            if tokens.get(body)?.0 != Token::Colon {
                return skip_single_statement(tokens, body);
            }
            let end = match tokens[start].0 {
                Token::While => Token::EndWhile,
                Token::For => Token::EndFor,
                Token::Foreach => Token::EndForeach,
                _ => Token::EndSwitch,
            };
            skip_alternative_block(tokens, body, &end)
        }
        Token::Eof => None,
        _ => skip_delimited_statement(tokens, start),
    }
}

/// Keeps elseif/else bodies with their nearest if, including nested unbraced conditionals.
fn skip_if_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let body = skip_group(tokens, start + 1, &Token::LParen, &Token::RParen)?;
    if tokens.get(body)?.0 == Token::Colon {
        return skip_alternative_block(tokens, body, &Token::EndIf);
    }
    let mut pos = skip_single_statement(tokens, body)?;
    while tokens.get(pos)?.0 == Token::ElseIf {
        let body = skip_group(tokens, pos + 1, &Token::LParen, &Token::RParen)?;
        pos = skip_single_statement(tokens, body)?;
    }
    if tokens.get(pos)?.0 == Token::Else {
        pos = skip_single_statement(tokens, pos + 1)?;
    }
    Some(pos)
}

/// Scans all catch clauses and an optional finally after the protected block.
fn skip_try_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = skip_group(tokens, start + 1, &Token::LBrace, &Token::RBrace)?;
    let mut has_handler = false;
    while tokens.get(pos)?.0 == Token::Catch {
        let body = skip_group(tokens, pos + 1, &Token::LParen, &Token::RParen)?;
        pos = skip_group(tokens, body, &Token::LBrace, &Token::RBrace)?;
        has_handler = true;
    }
    if tokens.get(pos)?.0 == Token::Finally {
        pos = skip_group(tokens, pos + 1, &Token::LBrace, &Token::RBrace)?;
        has_handler = true;
    }
    has_handler.then_some(pos)
}

/// Skips complete nested statements until the matching alternative-syntax closing keyword.
fn skip_alternative_block(
    tokens: &[SpannedToken],
    start: usize,
    end: &Token,
) -> Option<usize> {
    let mut pos = start + 1;
    loop {
        let token = &tokens.get(pos)?.0;
        if token == end {
            return skip_semicolon(tokens, pos + 1);
        }
        match token {
            Token::ElseIf if end == &Token::EndIf => {
                pos = skip_group(tokens, pos + 1, &Token::LParen, &Token::RParen)?;
                if tokens.get(pos)?.0 != Token::Colon {
                    return None;
                }
                pos += 1;
            }
            Token::Else if end == &Token::EndIf => {
                if tokens.get(pos + 1)?.0 != Token::Colon {
                    return None;
                }
                pos += 2;
            }
            Token::Case | Token::Default if end == &Token::EndSwitch => {
                pos = skip_case_label(tokens, pos + 1)?;
            }
            _ => pos = skip_single_statement(tokens, pos)?,
        }
    }
}

/// Finds the case-label delimiter without mistaking a ternary expression's colon for it.
fn skip_case_label(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = start;
    let mut ternaries = 0usize;
    loop {
        match &tokens.get(pos)?.0 {
            Token::LParen => pos = skip_group(tokens, pos, &Token::LParen, &Token::RParen)?,
            Token::LBracket => pos = skip_group(tokens, pos, &Token::LBracket, &Token::RBracket)?,
            Token::LBrace => pos = skip_group(tokens, pos, &Token::LBrace, &Token::RBrace)?,
            Token::Question => {
                ternaries += 1;
                pos += 1;
            }
            Token::Colon if ternaries > 0 => {
                ternaries -= 1;
                pos += 1;
            }
            Token::Colon | Token::Semicolon => return Some(pos + 1),
            Token::Eof => return None,
            _ => pos += 1,
        }
    }
}

/// Skips stacked attribute groups without parsing expressions or creating anonymous classes.
fn skip_attribute_groups(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut pos = start;
    while tokens.get(pos)?.0 == Token::AttrOpen {
        let mut depth = 1usize;
        pos += 1;
        while depth > 0 {
            match &tokens.get(pos)?.0 {
                Token::AttrOpen | Token::LBracket => depth += 1,
                Token::RBracket => depth -= 1,
                Token::Eof => return None,
                _ => {}
            }
            pos += 1;
        }
    }
    Some(pos)
}

/// Identifies declarations whose body-closing brace is also the statement boundary.
fn is_braced_declaration(tokens: &[SpannedToken], start: usize) -> bool {
    match tokens.get(start).map(|(token, _)| token) {
        Some(Token::Class | Token::Interface | Token::Trait | Token::Abstract
            | Token::Final | Token::ReadOnly | Token::Packed | Token::Namespace) => true,
        Some(Token::Enum) => tokens.get(start + 1).is_some_and(|(token, metadata)| {
            super::name_part_from_token(token, metadata).is_some()
        }),
        Some(Token::Function) => {
            let name = start + 1 + usize::from(
                matches!(tokens.get(start + 1).map(|(token, _)| token), Some(Token::Ampersand))
            );
            !matches!(tokens.get(name).map(|(token, _)| token), Some(Token::LParen))
        }
        _ => false,
    }
}

/// Preserves delimiter-aware boundaries for non-control statements and declarations.
fn skip_delimited_statement(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let braced_declaration = is_braced_declaration(tokens, start);
    let mut pos = start;
    let mut depth = 0usize;
    while let Some((token, _)) = tokens.get(pos) {
        match token {
            Token::LParen | Token::LBracket | Token::LBrace | Token::AttrOpen => depth += 1,
            Token::RParen | Token::RBracket => depth = depth.checked_sub(1)?,
            Token::RBrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && braced_declaration {
                    let next = pos + 1;
                    return Some(skip_semicolon(tokens, next).unwrap_or(next));
                }
            }
            Token::Semicolon if depth == 0 => return Some(pos + 1),
            Token::Eof => return None,
            _ => {}
        }
        pos += 1;
    }
    None
}
