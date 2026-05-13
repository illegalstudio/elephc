//! Purpose:
//! Parses PHP attribute groups such as `#[Name]`, `#[Name(args)]`,
//! comma-separated groups, and stacked groups.
//!
//! Called from:
//! - `crate::parser::stmt` and closure/parameter parsers.
//!
//! Key details:
//! - Declaration attributes are captured for later passes; parameter and
//!   closure attributes are currently syntax-validated and discarded.
//!
//! The grammar:
//!
//! ```text
//! attribute-group     = "#[" attribute ("," attribute)* "]"
//! attribute           = qualified-name [ "(" arg-list? ")" ]
//! qualified-name      = ["\"] identifier ("\" identifier)*
//! arg-list            = expr ("," expr)*
//! ```

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Attribute, AttributeGroup};
use crate::parser::expr::parse_args;
use crate::span::Span;

/// Parse zero or more `#[...]` attribute groups starting at `*pos`.
///
/// Each call walks any number of stacked groups (`#[A] #[B]`); each group
/// may contain several attributes separated by commas.
pub(crate) fn parse_attribute_lists(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Vec<AttributeGroup>, CompileError> {
    let mut groups = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 == Token::AttrOpen {
        groups.push(parse_one_group(tokens, pos)?);
    }
    Ok(groups)
}

/// Parse and discard attribute groups — used at sites where the AST does
/// not yet carry an `attributes` field (parameters, closure params).
pub(crate) fn consume_attribute_lists(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<(), CompileError> {
    parse_attribute_lists(tokens, pos).map(|_| ())
}

fn parse_one_group(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<AttributeGroup, CompileError> {
    let open_span = tokens[*pos].1;
    *pos += 1; // consume `#[`

    let mut attributes = Vec::new();
    let mut first = true;
    loop {
        if *pos >= tokens.len() {
            return Err(CompileError::new(
                open_span,
                "Unterminated attribute group: expected ']'",
            ));
        }
        if matches!(tokens[*pos].0, Token::RBracket) {
            if first {
                return Err(CompileError::new(
                    open_span,
                    "Empty attribute group: expected at least one attribute name",
                ));
            }
            *pos += 1; // consume `]`
            return Ok(AttributeGroup {
                attributes,
                span: open_span,
            });
        }
        if !first {
            if !matches!(tokens[*pos].0, Token::Comma) {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' or ']' between attributes",
                ));
            }
            *pos += 1; // consume `,`
            // Trailing comma before `]` is permitted (matches PHP).
            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::RBracket) {
                *pos += 1;
                return Ok(AttributeGroup {
                    attributes,
                    span: open_span,
                });
            }
        }
        attributes.push(parse_one_attribute(tokens, pos)?);
        first = false;
    }
}

fn parse_one_attribute(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Attribute, CompileError> {
    let span = tokens[*pos].1;
    let mut parts: Vec<String> = Vec::new();
    let mut fully_qualified = false;
    if matches!(tokens[*pos].0, Token::Backslash) {
        *pos += 1;
        fully_qualified = true;
    }
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(ident)) => {
            parts.push(ident.clone());
            *pos += 1;
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected attribute name (identifier)",
            ));
        }
    }
    while *pos < tokens.len() && matches!(tokens[*pos].0, Token::Backslash) {
        *pos += 1;
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(ident)) => {
                parts.push(ident.clone());
                *pos += 1;
            }
            _ => {
                return Err(CompileError::new(
                    tokens.get(*pos).map(|(_, s)| *s).unwrap_or(span),
                    "Expected identifier after '\\' in attribute name",
                ));
            }
        }
    }

    let kind = if fully_qualified {
        NameKind::FullyQualified
    } else if parts.len() > 1 {
        NameKind::Qualified
    } else {
        NameKind::Unqualified
    };
    let name = Name::from_parts(kind, parts);

    let mut args = Vec::new();
    if *pos < tokens.len() && matches!(tokens[*pos].0, Token::LParen) {
        let arg_span = tokens[*pos].1;
        *pos += 1;
        args = parse_args(tokens, pos, arg_span)?;
    }
    Ok(Attribute { name, args, span })
}
