//! Purpose:
//! Recovers namespace-qualified PHP declaration ranges for monitor source attribution.
//!
//! Called from:
//! - `super::exact` and `super::render` when reading a capture's source file.
//!
//! Key details:
//! - Lexer tokens exclude comments and keep braces in strings out of scope tracking.
//! - Scope boundaries use token positions, including multiple declarations on one line.
//! - Names retain source spelling so exact instrumentation can attach measured calls.
//! - Unlexable source yields no virtual frames rather than guessed declaration names.

use elephc::lexer::{SpannedToken, Token};

use super::DeclRange;

/// Extracts named function and method ranges without requiring a complete PHP AST.
pub(crate) fn php_decl_ranges(source: &str) -> Vec<DeclRange> {
    let Ok(tokens) = elephc::lexer::tokenize(source) else { return Vec::new(); };
    let brace_ends = matching_brace_ends(&tokens);
    let mut ranges = Vec::new();
    let mut classes: Vec<(String, usize)> = Vec::new();
    let mut namespace = String::new();
    let mut namespace_end = None;
    let mut namespace_stack = Vec::new();
    let mut pos = 0;
    while pos < tokens.len() {
        while namespace_end.is_some_and(|end| pos > end) {
            (namespace, namespace_end) = namespace_stack.pop().unwrap_or_default();
        }
        while classes.last().is_some_and(|(_, end)| pos > *end) {
            classes.pop();
        }
        if tokens[pos].0 == Token::Namespace && declaration_boundary(&tokens, pos) {
            if let Some((name, delimiter)) = namespace_declaration(&tokens, pos + 1) {
                let braced = tokens[delimiter].0 == Token::LBrace;
                if braced || namespace_end.is_some() {
                    namespace_stack.push((namespace, namespace_end));
                }
                namespace = name;
                if braced {
                    namespace_end = Some(brace_ends[delimiter].unwrap_or(tokens.len() - 1));
                }
                pos = delimiter + 1;
                continue;
            }
        }
        if matches!(tokens[pos].0, Token::Class | Token::Interface | Token::Trait | Token::Enum) {
            if let Some(name) = tokens.get(pos + 1).and_then(|(token, metadata)| {
                elephc::parser::name_part_from_token(token, metadata)
            }) {
                if let Some((open, end)) = declaration_extent(&tokens, pos + 2, &brace_ends, false) {
                    classes.push((qualify_name(&namespace, &name), end));
                    pos = open + 1;
                    continue;
                }
            } else if tokens[pos].0 == Token::Class && (
                pos > 0 && tokens[pos - 1].0 == Token::New
                || pos > 1 && tokens[pos - 1].0 == Token::ReadOnly && tokens[pos - 2].0 == Token::New
            ) {
                // Anonymous-class names belong to the parser, not to source tokens.
                // Skip their methods instead of inventing free-function virtual frames.
                if let Some((_, end)) = declaration_extent(&tokens, pos + 1, &brace_ends, false) {
                    pos = end + 1;
                    continue;
                }
            }
        }
        if tokens[pos].0 == Token::Function {
            let name_pos = pos + 1 + usize::from(tokens.get(pos + 1).is_some_and(|(token, _)| *token == Token::Ampersand));
            if let Some(name) = word_at(&tokens, name_pos) {
                if let Some(params_pos) = parameter_list_start(&tokens, name_pos + 1) {
                    if let Some((_, end)) = declaration_extent(&tokens, params_pos, &brace_ends, true) {
                        let name = match classes.last() {
                            Some((class, _)) => format!("{class}::{name}"),
                            None => qualify_name(&namespace, &name),
                        };
                        ranges.push(DeclRange {
                            name, start: tokens[pos].1.span.line, end: tokens[end].1.span.end_line,
                        });
                        // A nested closure must not shadow the named function's virtual frame.
                        pos = end + 1;
                        continue;
                    }
                }
            }
        }
        pos += 1;
    }
    ranges
}

/// Matches structural braces once; literal strings contain no structural brace tokens.
fn matching_brace_ends(tokens: &[SpannedToken]) -> Vec<Option<usize>> {
    let mut ends = vec![None; tokens.len()];
    let mut stack = Vec::new();
    for (pos, (token, _)) in tokens.iter().enumerate() {
        match token {
            Token::LBrace => stack.push(pos),
            Token::RBrace => {
                if let Some(open) = stack.pop() { ends[open] = Some(pos); }
            }
            _ => {}
        }
    }
    ends
}

/// Excludes keyword fragments in imports and relative namespace expressions from declarations.
fn declaration_boundary(tokens: &[SpannedToken], pos: usize) -> bool {
    pos == 0 || matches!(tokens[pos - 1].0,
        Token::OpenTag | Token::Semicolon | Token::LBrace | Token::RBrace)
}

/// Reads an actual namespace name and its delimiter, independent of source line breaks.
fn namespace_declaration(tokens: &[SpannedToken], mut pos: usize) -> Option<(String, usize)> {
    if tokens.get(pos)?.0 == Token::LBrace { return Some((String::new(), pos)); }
    let mut name = String::new();
    loop {
        name.push_str(&word_at(tokens, pos)?);
        pos += 1;
        if tokens.get(pos)?.0 != Token::Backslash { break; }
        name.push('\\');
        pos += 1;
    }
    matches!(tokens.get(pos)?.0, Token::Semicolon | Token::LBrace).then_some((name, pos))
}

/// Returns the source spelling of an identifier or contextual keyword used as a name.
fn word_at(tokens: &[SpannedToken], pos: usize) -> Option<String> {
    let (token, metadata) = tokens.get(pos)?;
    token.word_spelling(metadata).map(str::to_string)
}

/// Skips a balanced generic declaration list before the source-visible parameters.
fn parameter_list_start(tokens: &[SpannedToken], mut pos: usize) -> Option<usize> {
    if tokens.get(pos)?.0 == Token::Less {
        let mut depth = 0usize;
        loop {
            match tokens.get(pos)?.0 {
                Token::Less => depth += 1,
                Token::Greater => depth = depth.checked_sub(1)?,
                Token::GreaterGreater => depth = depth.checked_sub(2)?,
                Token::Semicolon | Token::LBrace | Token::RBrace | Token::Eof => return None,
                _ => {}
            }
            pos += 1;
            if depth == 0 { break; }
        }
    }
    (tokens.get(pos)?.0 == Token::LParen).then_some(pos)
}

/// Locates a declaration's body or abstract-method delimiter after its complete header.
fn declaration_extent(
    tokens: &[SpannedToken],
    start: usize,
    brace_ends: &[Option<usize>],
    allow_semicolon: bool,
) -> Option<(usize, usize)> {
    let mut header_depth = 0usize;
    for (pos, (token, _)) in tokens.iter().enumerate().skip(start) {
        match token {
            Token::LParen | Token::LBracket | Token::AttrOpen => header_depth += 1,
            Token::RParen | Token::RBracket => header_depth = header_depth.saturating_sub(1),
            Token::LBrace if header_depth == 0 => {
                return Some((pos, brace_ends[pos].unwrap_or(tokens.len() - 1)));
            }
            Token::Semicolon if header_depth == 0 => return allow_semicolon.then_some((pos, pos)),
            Token::Eof => break,
            _ => {}
        }
    }
    None
}

/// Keeps declaration names in the namespace-qualified form used by demangled symbols.
fn qualify_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() { name.to_string() } else { format!("{namespace}\\{name}") }
}
