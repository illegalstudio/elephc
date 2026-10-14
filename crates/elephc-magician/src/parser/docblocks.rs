//! Purpose:
//! Adapts supported generic PHPDoc annotations into the existing native type grammar.
//!
//! Called from:
//! - `Parser::new` before declaration parsing.
//!
//! Key details:
//! - Synthesized tokens retain PHPDoc provenance so strict PHP mode accepts annotations.
//! - Unsupported annotations are ignored; written native templates take precedence.

use crate::lexer::{Token, TokenKind};
use std::collections::HashMap;

/// Supported annotation syntax before type parsing and lexical name resolution.
#[derive(Default)]
struct Block {
    templates: Vec<(String, String)>,
    params: HashMap<String, String>,
    returns: Option<String>,
    var: Option<String>,
    inherited: Vec<(&'static str, String)>,
}

/// Reads the PHPStan/Psalm tags already supported by the AOT frontend.
fn block(comment: &str) -> Block {
    let mut block = Block::default();
    for line in comment.lines() {
        let line = elephc_generics::strip_comment_markers(line);
        if let Some((rest, variance)) = elephc_generics::strip_template_tag(&line) {
            let mut pieces = rest.splitn(2, char::is_whitespace);
            let name = pieces.next().unwrap_or("");
            if name.is_empty() || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') { continue; }
            let remaining = pieces.next().unwrap_or("").trim();
            let remaining = remaining.strip_prefix("of ").map_or(remaining.to_string(), |bound| format!(": {bound}"));
            let marker = if variance > 0 { "+" } else if variance < 0 { "-" } else { "" };
            let syntax = format!("{marker}{name} {remaining}");
            let mut parser = super::state::Parser::new(annotation_tokens(&format!("<{syntax}>"), 1), 0);
            if parser.parse_generic_parameters().is_ok() && !block.templates.iter().any(|(existing, _)| existing == name) {
                block.templates.push((name.into(), syntax));
            }
        } else if let Some(rest) = line.strip_prefix("@param ") {
            if let Some((ty, name)) = rest.split_once('$') {
                let name = name.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect::<String>();
                let ty = ty.trim().trim_end_matches("...").trim();
                if valid_type(ty) { block.params.insert(name, ty.into()); }
            }
        } else if let Some(rest) = line.strip_prefix("@return ") {
            if valid_type(rest.trim()) { block.returns = Some(rest.trim().into()); }
        } else if let Some(rest) = line.strip_prefix("@var ") {
            let ty = rest.split('$').next().unwrap_or("").trim();
            if valid_type(ty) { block.var = Some(ty.into()); }
        } else if let Some((clause, rest)) = line.strip_prefix("@extends ").map(|rest| ("extends", rest))
            .or_else(|| line.strip_prefix("@implements ").map(|rest| ("implements", rest))) {
            if valid_type(rest.trim()) && rest.contains('<') { block.inherited.push((clause, rest.trim().into())); }
        }
    }
    block
}

/// Accepts only complete types supported by the shared native type grammar.
fn valid_type(source: &str) -> bool { super::generics::parse_type_string(source).is_ok() }

/// Produces line-preserving tokens marked as portable annotation input.
fn annotation_tokens(source: &str, line: i64) -> Vec<Token> {
    crate::lexer::tokenize(source).unwrap_or_default().into_iter()
        .filter(|token| !matches!(token.kind(), TokenKind::Eof))
        .map(|token| Token::new(token.into_kind(), line).with_phpdoc(true)).collect()
}

/// Finds the matching close of a parenthesis or brace pair.
fn matching(tokens: &[Token], start: usize, open: TokenKind, close: TokenKind) -> Option<usize> {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind() == &open { depth += 1; }
        if token.kind() == &close { depth -= 1; if depth == 0 { return Some(index); } }
    }
    None
}

/// Determines whether a type annotation names any enclosing class template slot.
fn mentions(source: &str, names: &[String]) -> bool {
    crate::lexer::tokenize(source).is_ok_and(|tokens| tokens.iter().any(|token| matches!(token.kind(), TokenKind::Ident(name) if names.contains(name))))
}

/// Restricts property annotations to the direct body of a class-like declaration.
fn direct_member(tokens: &[Token], position: usize, scopes: &[(usize, usize, Vec<String>)]) -> bool {
    let Some((start, _, _)) = scopes.iter().filter(|(start, end, _)| position > *start && position < *end).max_by_key(|(start, _, _)| *start) else { return false; };
    let mut depth = 0;
    for token in &tokens[*start..position] {
        match token.kind() { TokenKind::LBrace => depth += 1, TokenKind::RBrace => depth -= 1, _ => {} }
    }
    depth == 1
}

/// Skips declaration attributes and modifiers while retaining them in annotation edits.
fn skip_prefix(tokens: &[Token], mut position: usize, limit: usize) -> usize {
    while position < limit {
        if matches!(tokens[position].kind(), TokenKind::AttributeStart) {
            let mut depth = 1;
            position += 1;
            while position < limit && depth != 0 {
                match tokens[position].kind() {
                    TokenKind::LBracket | TokenKind::AttributeStart => depth += 1,
                    TokenKind::RBracket => depth -= 1,
                    _ => {}
                }
                position += 1;
            }
        } else if matches!(tokens[position].kind(), TokenKind::Ident(name) if ["public", "protected", "private", "static", "readonly", "abstract", "final"].contains(&name.to_ascii_lowercase().as_str())) {
            position += 1;
            if matches!(tokens.get(position).map(Token::kind), Some(TokenKind::LParen))
                && matches!(tokens.get(position + 1).map(Token::kind), Some(TokenKind::Ident(name)) if name == "set")
                && matches!(tokens.get(position + 2).map(Token::kind), Some(TokenKind::RParen)) { position += 3; }
        } else { break; }
    }
    position
}

/// Selects each parent once by its complete written name and extends/implements clause.
fn inheritance_edits(tokens: &[Token], start: usize, end: usize, annotations: &[(&str, String)], line: i64) -> Vec<(usize, usize, Vec<Token>)> {
    let mut edits = Vec::new();
    let mut clause = "";
    let mut parent = false;
    let mut depth = 0;
    let mut position = start;
    while position < end {
        match tokens[position].kind() {
            TokenKind::Less => depth += 1,
            TokenKind::Greater => depth -= 1,
            TokenKind::GreaterGreater => depth -= 2,
            TokenKind::Ident(name) if depth == 0 && (name.eq_ignore_ascii_case("extends") || name.eq_ignore_ascii_case("implements")) => {
                clause = if name.eq_ignore_ascii_case("extends") { "extends" } else { "implements" };
                parent = true;
            }
            TokenKind::Comma if depth == 0 && !clause.is_empty() => parent = true,
            TokenKind::Ident(_) | TokenKind::Backslash if depth == 0 && parent => {
                let mut name = String::new();
                if matches!(tokens[position].kind(), TokenKind::Backslash) { name.push('\\'); position += 1; }
                while position < end {
                    let TokenKind::Ident(segment) = tokens[position].kind() else { break; };
                    name.push_str(segment);
                    position += 1;
                    if !matches!(tokens.get(position).map(Token::kind), Some(TokenKind::Backslash)) { break; }
                    name.push('\\');
                    position += 1;
                }
                if !matches!(tokens.get(position).map(Token::kind), Some(TokenKind::Less)) {
                    if let Some((_, annotation)) = annotations.iter().find(|(kind, annotation)| *kind == clause
                        && annotation.split('<').next().is_some_and(|base| base.trim().eq_ignore_ascii_case(&name))) {
                        let args = &annotation[annotation.find('<').unwrap()..];
                        edits.push((position, position, annotation_tokens(args, line)));
                    }
                }
                parent = false;
                continue;
            }
            _ => {}
        }
        position += 1;
    }
    edits
}

/// Rewrites only declaration annotations, leaving doc comments inside expressions inert.
pub(super) fn apply(tokens: Vec<Token>) -> Vec<Token> {
    if !tokens.iter().any(|token| matches!(token.kind(), TokenKind::DocComment(_))) { return tokens; }
    let mut edits: Vec<(usize, usize, Vec<Token>)> = Vec::new();
    let mut scopes: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.kind(), TokenKind::Ident(name) if ["class", "interface", "trait"].iter().any(|keyword| name.eq_ignore_ascii_case(keyword))) { continue; }
        let header = index + 2;
        if !matches!(tokens.get(header).map(Token::kind), Some(TokenKind::Less)) { continue; }
        let Some(body) = (header..tokens.len()).find(|index| matches!(tokens[*index].kind(), TokenKind::LBrace)) else { continue; };
        let Some(end) = matching(&tokens, body, TokenKind::LBrace, TokenKind::RBrace) else { continue; };
        let mut parser = super::state::Parser::new(tokens[header..body].iter().filter(|token| !matches!(token.kind(), TokenKind::DocComment(_))).cloned().collect(), 0);
        if let Ok(parameters) = parser.parse_generic_parameters() { scopes.push((body, end, parameters.into_iter().map(|param| param.name).collect())); }
    }
    for (comment_index, token) in tokens.iter().enumerate() {
        let TokenKind::DocComment(comment) = token.kind() else { continue; };
        edits.push((comment_index, comment_index + 1, Vec::new()));
        let block = block(comment);
        let declaration = skip_prefix(&tokens, comment_index + 1, tokens.len());
        let Some(next) = tokens.get(declaration) else { continue; };
        let class_like = matches!(next.kind(), TokenKind::Ident(name) if ["class", "interface", "trait"].contains(&name.to_ascii_lowercase().as_str()));
        let function = matches!(next.kind(), TokenKind::Ident(name) if name.eq_ignore_ascii_case("function"));
        let in_scope = scopes.iter().filter(|(start, end, _)| declaration > *start && declaration < *end).flat_map(|(_, _, names)| names.clone()).collect::<Vec<_>>();
        if class_like {
            let header = declaration + 2;
            let native = tokens.get(header).is_some_and(|token| matches!(token.kind(), TokenKind::Less));
            let Some(body) = (header..tokens.len()).find(|index| matches!(tokens[*index].kind(), TokenKind::LBrace)) else { continue; };
            if let Some(end) = matching(&tokens, body, TokenKind::LBrace, TokenKind::RBrace).filter(|_| !native) {
                scopes.push((body, end, block.templates.iter().map(|(name, _)| name.clone()).collect()));
            }
            if !native && !block.templates.is_empty() {
                let params = block.templates.iter().map(|(_, syntax)| syntax.clone()).collect::<Vec<_>>().join(", ");
                edits.push((header, header, annotation_tokens(&format!("<{params}>"), token.line())));
            }
            edits.extend(inheritance_edits(&tokens, header, body, &block.inherited, token.line()));
        } else if function {
            let header = declaration + 2 + usize::from(matches!(tokens.get(declaration + 1).map(Token::kind), Some(TokenKind::Ampersand)));
            if tokens.get(header).is_some_and(|token| matches!(token.kind(), TokenKind::Less)) { continue; }
            let own = !block.templates.is_empty();
            if own {
                let params = block.templates.iter().map(|(_, syntax)| syntax.clone()).collect::<Vec<_>>().join(", ");
                edits.push((header, header, annotation_tokens(&format!("<{params}>"), token.line())));
            }
            if !matches!(tokens.get(header).map(Token::kind), Some(TokenKind::LParen)) { continue; }
            let Some(end) = matching(&tokens, header, TokenKind::LParen, TokenKind::RParen) else { continue; };
            let mut parameter_start = header + 1;
            let mut nesting = 0i32;
            for index in header + 1..end {
                match tokens[index].kind() {
                    TokenKind::LParen | TokenKind::LBracket | TokenKind::AttributeStart | TokenKind::Less => nesting += 1,
                    TokenKind::RParen | TokenKind::RBracket | TokenKind::Greater => nesting -= 1,
                    TokenKind::GreaterGreater => nesting -= 2,
                    TokenKind::Comma if nesting == 0 => parameter_start = index + 1,
                    TokenKind::DollarIdent(name) if nesting == 0 => {
                        if let Some(ty) = block.params.get(name).filter(|ty| own || mentions(ty, &in_scope)) {
                            let type_start = skip_prefix(&tokens, parameter_start, index);
                            let mut type_end = index;
                            while type_end > type_start && matches!(tokens[type_end - 1].kind(), TokenKind::Ampersand | TokenKind::Ellipsis) { type_end -= 1; }
                            edits.push((type_start, type_end, annotation_tokens(ty, token.line())));
                        }
                    }
                    _ => {}
                }
            }
            if let Some(ty) = block.returns.as_ref().filter(|ty| own || mentions(ty, &in_scope)) {
                let start = end + 1;
                let finish = (start..tokens.len()).find(|index| matches!(tokens[*index].kind(), TokenKind::LBrace | TokenKind::Semicolon | TokenKind::FatArrow)).unwrap_or(start);
                edits.push((start, finish, annotation_tokens(&format!(": {ty}"), token.line())));
            }
        } else if let Some(ty) = block.var.as_ref().filter(|ty| mentions(ty, &in_scope) && direct_member(&tokens, declaration, &scopes)) {
            if let Some(variable) = (declaration..tokens.len()).take_while(|index| !matches!(tokens[*index].kind(), TokenKind::Semicolon | TokenKind::LBrace))
                .find(|index| matches!(tokens[*index].kind(), TokenKind::DollarIdent(_))) {
                edits.push((declaration, variable, annotation_tokens(ty, token.line())));
            }
        }
    }
    edits.sort_by_key(|(start, end, _)| (*start, *end));
    let mut output = Vec::new();
    let mut position = 0;
    for (start, end, replacement) in edits {
        if start < position { continue; }
        output.extend_from_slice(&tokens[position..start]);
        output.extend(replacement);
        position = end;
    }
    output.extend_from_slice(&tokens[position..]);
    output
}
