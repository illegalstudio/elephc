//! Purpose:
//! Associates PHPDoc with individual declaration tokens instead of whole source lines.
//! Recovers comments only between lexer tokens, excluding strings and ordinary comments.
//!
//! Called from:
//! - `crate::docblock::collect()` before applying annotations to a physical file's AST.
//!
//! Key details:
//! - Keys retain both line and column, so members cannot adopt their owner's docblock.
//! - Attribute groups are skipped with token-balanced brackets.
//! - Property declarators also get their declaration's block because their spans start at `$`.

use std::collections::HashMap;

use crate::lexer::{tokenize, tokenize_with_mode, SpannedToken, Token};
use crate::source::SourceMode;

use super::{parse_block, DocBlock};

/// Collects annotated declaration positions, including property declarator positions.
pub(super) fn collect(source: &str) -> HashMap<(u32, u32), DocBlock> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut blocks = HashMap::new();
    let Ok(tokens) = tokenize(source).or_else(|_| tokenize_with_mode(source, SourceMode::Lfc)) else {
        return blocks;
    };
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let columns = (!line.is_ascii()).then(|| line.char_indices().map(|(index, _)| index).collect());
        lines.push((offset, line.len(), columns));
        offset += line.len();
    }
    lines.push((source.len(), 0, None));
    let mut previous_end = 0;
    for (index, (token, meta)) in tokens.iter().enumerate() {
        let start = source_offset(&lines, meta.span.line, meta.span.col);
        if start >= previous_end {
            if let Some(comment) = last_docblock(&source[previous_end..start]) {
                let block = parse_block(&comment.lines().collect::<Vec<_>>());
                if !block.is_empty() {
                    if let Some(target) = declaration_after_attributes(&tokens, index) {
                        let span = tokens[target].1.span;
                        blocks.insert((span.line, span.col), block.clone());
                        // Property AST spans start at each variable, after modifiers and hints.
                        for (token, meta) in &tokens[target..] {
                            if matches!(token, Token::Function | Token::Class | Token::Interface
                                | Token::Trait | Token::Enum | Token::LParen | Token::LBrace
                                | Token::Semicolon | Token::Eof)
                            {
                                break;
                            }
                            if matches!(token, Token::Variable(_)) {
                                blocks.insert((meta.span.line, meta.span.col), block.clone());
                            }
                        }
                    }
                }
            }
        }
        if *token == Token::OpenTag && source[start..].starts_with("<?php") {
            // The opening tag has a point span rather than the ordinary token extent.
            previous_end = start + 5;
        } else if *token != Token::Eof {
            previous_end = previous_end.max(source_offset(
                &lines, meta.span.end_line, meta.span.end_column(),
            ));
        }
    }
    blocks
}

/// Converts the lexer's character coordinates to a byte offset in the original source.
fn source_offset(lines: &[(usize, usize, Option<Vec<usize>>)], line: u32, col: u32) -> usize {
    let (offset, len, columns) = &lines[line as usize - 1];
    offset + columns.as_ref().map_or(col as usize - 1, |columns| {
        columns.get(col as usize - 1).copied().unwrap_or(*len)
    })
}

/// Finds the last real docblock in a gap made of whitespace and comments.
fn last_docblock(mut gap: &str) -> Option<&str> {
    let mut last = None;
    loop {
        gap = gap.trim_start();
        if gap.is_empty() {
            return last;
        }
        if gap.starts_with("//") || gap.starts_with('#') {
            gap = gap.find('\n').map_or("", |end| &gap[end + 1..]);
        } else if gap.starts_with("/*") {
            // The opener is consumed before the closer is searched for, as php's lexer does: in
            // `/*/ kept */` the `*` of `/*` and the `/` after it are not a closer, so finding `*/`
            // from the start ended the comment at its third byte and read the rest as code.
            let end = gap[2..].find("*/")? + 4;
            // Only a docblock replaces the one already seen. An ordinary `/* … */` between the
            // docblock and its declaration, on one line or several, is skipped like `//` is; it
            // used to clear `last`, and the declaration lost its `@template`.
            if is_docblock(gap) {
                last = Some(&gap[..end]);
            }
            gap = &gap[end..];
        } else {
            return None;
        }
    }
}

/// Returns whether a comment starting `gap` is a docblock as php's lexer defines one.
///
/// `T_DOC_COMMENT` is `/**` followed by a space, tab, LF or CR. `/***` banners and the empty
/// `/**/` are ordinary `T_COMMENT`s, and `ReflectionFunction::getDocComment()` skips them, so they
/// must not replace the real docblock above them.
fn is_docblock(gap: &str) -> bool {
    // php's rule is `"/**"[ \n\r\t]`: a form feed, a vertical tab or a non-breaking space after
    // the opener makes an ordinary comment, which `getDocComment()` ignores too.
    gap.strip_prefix("/**")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|next| matches!(next, ' ' | '\t' | '\n' | '\r'))
}

/// Skips consecutive attribute groups and returns the declaration's first token.
fn declaration_after_attributes(tokens: &[SpannedToken], mut index: usize) -> Option<usize> {
    while tokens.get(index)?.0 == Token::AttrOpen {
        index += 1;
        let mut depth = 1;
        while depth > 0 {
            match tokens.get(index)?.0 {
                Token::AttrOpen | Token::LBracket => depth += 1,
                Token::RBracket => depth -= 1,
                Token::Eof => return None,
                _ => {}
            }
            index += 1;
        }
    }
    (tokens.get(index)?.0 != Token::Eof).then_some(index)
}
