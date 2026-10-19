//! Purpose:
//! Lexes PHP inline HTML (the text outside `<?php ... ?>`) into tokens the grammar already has.
//!
//! Called from:
//! - `crate::lexer::scan::scan_tokens_in_source()` at the start of a PHP-mode file and after
//!   every `?>` close tag.
//!
//! Key details:
//! - The text before the first open tag and between `?>` and the next open tag becomes
//!   `echo "text";`; `<?=` becomes `echo`; the `?>` close tag becomes `;` (as in PHP's grammar,
//!   where `?>` is an implicit `;`). A control structure that spans the tags
//!   (`if (...): ?>HTML<?php endif;`) therefore parses with no parser support.
//! - One line break right after `?>` (`\n`, `\r\n` or `\r`) belongs to the tag and is not output.
//! - `<?php` opens code only when followed by a space, tab, line break or the end of the file
//!   (case-insensitive). The short `<?` tag is plain text: `short_open_tag` is off in
//!   production php.ini files. This mirrors `elephc-magician`'s eval lexer.
//! - Every emitted token carries a span, so line numbers after inline HTML stay exact; the HTML
//!   string token spans the whole text so docblock lookups never see its contents as code.

use super::cursor::Cursor;
use super::token::{spanned, SpannedToken, Token};
use crate::span::Span;

/// Consumes the close tag at the cursor (`?>`) and the single line break it swallows.
pub(super) fn consume_close_tag(cursor: &mut Cursor) {
    cursor.advance();
    cursor.advance();
    match cursor.peek() {
        Some('\r') => {
            cursor.advance();
            if cursor.peek() == Some('\n') {
                cursor.advance();
            }
        }
        Some('\n') => {
            cursor.advance();
        }
        _ => {}
    }
}

/// Scans inline HTML up to and including the next open tag, pushing `echo "html";` for any
/// non-empty text and a trailing `echo` for a `<?=` tag. Stops at the end of the source when
/// no further tag exists.
pub(super) fn scan_inline_html(cursor: &mut Cursor, tokens: &mut Vec<SpannedToken>) {
    let start_span = cursor.span();
    let rest = cursor.remaining();
    let mut search_from = 0;
    let mut open: Option<(usize, usize, bool)> = None;
    while let Some(found) = rest[search_from..].find("<?") {
        let at = search_from + found;
        let after = &rest[at + 2..];
        if after.starts_with('=') {
            open = Some((at, 3, true));
            break;
        }
        let bytes = after.as_bytes();
        if bytes.len() >= 3
            && bytes[..3].eq_ignore_ascii_case(b"php")
            && bytes.get(3).is_none_or(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            open = Some((at, 5, false));
            break;
        }
        search_from = at + 2;
    }
    let (html_len, tag_len, is_echo) = match open {
        Some((at, tag_len, is_echo)) => (at, tag_len, is_echo),
        None => (rest.len(), 0, false),
    };
    if html_len > 0 {
        let html = rest[..html_len].to_string();
        for _ in html.chars() {
            cursor.advance();
        }
        let span = Span::with_end_from(start_span, cursor.span());
        tokens.push(spanned(Token::Echo, span));
        tokens.push(spanned(Token::StringLiteral(html), span));
        tokens.push(spanned(Token::Semicolon, span));
    }
    if tag_len > 0 {
        let tag_span = cursor.span();
        for _ in 0..tag_len {
            cursor.advance();
        }
        if is_echo {
            tokens.push(spanned(Token::Echo, tag_span));
        }
    }
}
