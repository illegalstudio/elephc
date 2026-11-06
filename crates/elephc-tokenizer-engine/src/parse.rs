//! Purpose:
//! Implements TOKEN_PARSE syntax validation and parser feedback for reserved identifiers.
//!
//! Called from:
//! - `crate::tokenize` only when TOKEN_PARSE is set.
//!
//! Key details:
//! - Parsing uses PHP's grammar rather than elephc's supported source subset.
//! - Feedback visits genuine identifier nodes, leaving magic constants and ::class intact.

use std::collections::HashSet;
use mago_allocator::LocalArena;
use mago_database::file::FileId;
use mago_span::HasSpan;
use mago_syntax::{cst::Node, parser::parse_file_content_with_settings, settings::{LexerSettings, ParserSettings}};
use crate::{id, newline_count, ParseError, Token};

/// Validates PHP source and changes semi-reserved identifier tokens to T_STRING.
pub(super) fn validate(source: &[u8], tokens: &mut [Token]) -> Result<(), ParseError> {
    let mut input = super::lex::normalize_tags(source);
    // Mago's halt state does not accept comments between the three delimiter tokens.
    // Mask only trivia in that header; Zend permits it and ignores the later payload.
    let mut in_halt = false;
    for token in tokens.iter() {
        // Zend's soft-keyword lookahead excludes extends/implements prefixes. Mago
        // accepts them as enum names, so preserve Zend's identifier classification.
        if token.id == id("T_STRING") && source[token.pos..token.pos + token.len].eq_ignore_ascii_case(b"enum") {
            input[token.pos] = b'z';
        }
        if token.id == id("T_HALT_COMPILER") { in_halt = true; }
        else if in_halt && [id("T_COMMENT"), id("T_DOC_COMMENT")].contains(&token.id) {
            for b in &mut input[token.pos..token.pos + token.len] { if !matches!(b, b'\n' | b'\r') { *b = b' '; } }
        }
        if token.id == id("T_DOUBLE_CAST") && source[token.pos..token.pos + token.len].windows(4).any(|s| s.eq_ignore_ascii_case(b"real")) {
            return Err(ParseError { message: "The (real) cast has been removed, use (float) instead".into(), line: token.line });
        }
    }
    let arena = LocalArena::new();
    let program = parse_file_content_with_settings(&arena, FileId::zero(), &input, ParserSettings {
        lexer: LexerSettings { enable_short_tags: false }, ..ParserSettings::default()
    });
    if let Some(error) = program.errors.first() {
        let at = (error.span().start.offset as usize).min(source.len());
        let message = if at == source.len() { "syntax error, unexpected end of file".to_owned() }
        else {
            let token = tokens.iter().find(|t| t.pos <= at && at < t.pos + t.len);
            match token {
                Some(t) if t.id == id("T_STRING") => format!("syntax error, unexpected identifier \"{}\"", String::from_utf8_lossy(&source[t.pos..t.pos + t.len])),
                Some(t) if t.len <= 3 => format!("syntax error, unexpected token \"{}\"", String::from_utf8_lossy(&source[t.pos..t.pos + t.len])),
                Some(t) => format!("syntax error, unexpected token \"{}\"", crate::token_name(t.id).unwrap_or("UNKNOWN")),
                None => "syntax error, unexpected end of file".to_owned(),
            }
        };
        return Err(ParseError { message, line: 1 + newline_count(&source[..at]) });
    }
    let mut names = HashSet::new();
    let mut invalid_echo = None;
    let mut invalid_escape = None;
    collect_names(Node::Program(program), &mut names, &mut invalid_echo, &mut invalid_escape);
    if let Some((at, message)) = invalid_escape {
        return Err(ParseError { message: message.into(), line: 1 + newline_count(&source[..at.min(source.len())]) });
    }
    if let Some(at) = invalid_echo {
        let at = at.min(source.len());
        let message = if at == source.len() { "syntax error, unexpected end of file".into() }
            else { format!("syntax error, unexpected token \"{}\"", String::from_utf8_lossy(&source[at..at + 1])) };
        return Err(ParseError { message, line: 1 + newline_count(&source[..at]) });
    }
    for token in tokens {
        if token.id >= 256 && token.id != id("T_STRING_VARNAME") && names.contains(&token.pos) { token.id = id("T_STRING"); }
    }
    Ok(())
}

/// Collects local identifier positions, skipping AST nodes with dedicated PHP token kinds.
fn collect_names(
    node: Node<'_, '_>, names: &mut HashSet<usize>, invalid_echo: &mut Option<usize>,
    invalid_escape: &mut Option<(usize, &'static str)>,
) {
    use mago_syntax::cst::{DocumentKind, LiteralStringKind};
    if matches!(node, Node::DocumentString(document) if document.kind == DocumentKind::Nowdoc) { return; }
    let literal = match node {
        Node::LiteralString(literal) if literal.kind == LiteralStringKind::DoubleQuoted => Some((literal.raw, literal.span.start.offset)),
        Node::LiteralStringPart(part) => Some((part.raw, part.span.start.offset)),
        _ => None,
    };
    if let Some((raw, at)) = literal {
        if let Some(message) = invalid_unicode_escape(raw) {
            invalid_escape.get_or_insert((at as usize, message));
        }
    }
    // Reserved words such as C::ECHO are identifiers here, not echo statements.
    let empty_or_trailing_echo = match node {
        Node::Echo(echo) if echo.values.nodes.is_empty() || echo.values.tokens.len() >= echo.values.nodes.len() => Some(echo.terminator.span()),
        Node::EchoTag(echo) if echo.values.nodes.is_empty() || echo.values.tokens.len() >= echo.values.nodes.len() => Some(echo.terminator.span()),
        _ => None,
    };
    if let Some(span) = empty_or_trailing_echo { invalid_echo.get_or_insert(span.start.offset as usize); }
    if matches!(node, Node::MagicConstant(_)) { return; }
    if let Node::LocalIdentifier(identifier) = node {
        names.insert(identifier.span.start.offset as usize);
        return;
    }
    node.visit_children(|child| collect_names(child, names, invalid_echo, invalid_escape));
    if let Node::FunctionCall(call) = node {
        if let mago_syntax::cst::Expression::Identifier(mago_syntax::cst::Identifier::Local(identifier)) = call.function {
            if identifier.value.eq_ignore_ascii_case(b"clone") { names.remove(&(identifier.span.start.offset as usize)); }
        }
    }
}

/// Validates PHP's active Unicode escapes without decoding or changing the original bytes.
fn invalid_unicode_escape(raw: &[u8]) -> Option<&'static str> {
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != b'\\' { i += 1; continue; }
        if raw.get(i + 1..i + 3) != Some(b"u{") { i += 2; continue; }
        let start = i + 3;
        let mut end = start;
        let mut codepoint = Some(0u64);
        while let Some(digit) = raw.get(end).and_then(|b| (*b as char).to_digit(16)) {
            codepoint = codepoint.and_then(|n| n.checked_mul(16)).and_then(|n| n.checked_add(u64::from(digit)));
            end += 1;
        }
        if start == end || raw.get(end) != Some(&b'}') { return Some("Invalid UTF-8 codepoint escape sequence"); }
        if codepoint.is_none_or(|n| n > 0x10ffff) { return Some("Invalid UTF-8 codepoint escape sequence: Codepoint too large"); }
        i = end + 1;
    }
    None
}
