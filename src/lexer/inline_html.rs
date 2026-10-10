//! Purpose:
//! Scans a `?>` close tag and the inline HTML after it, and leading inline HTML before the
//! first open tag, into tokens the existing grammar already consumes: the close tag is a
//! `;`, the HTML is `echo "…";`, and `<?=` is `echo`.
//!
//! Called from:
//! - `crate::lexer::scan::scan_tokens_in_source`, in PHP source mode only.
//!
//! Key details:
//! - A `?>` switches the scanner from PHP to HTML until the next open tag, exactly as in a
//!   file: `<?php echo "a"; ?>HTML<?php echo "b"; ?>` prints `aHTMLb`.
//! - One newline right after `?>` is swallowed (`\n`, `\r\n` or `\r`), as the Zend scanner's
//!   close-tag rule does.
//! - `<?php` opens code only when followed by a space, a tab, a line break, or the end of
//!   input (`<?phpX` is HTML), case-insensitively; the short `<?` tag is HTML because
//!   `short_open_tag` is off.
//! - The rules mirror the eval lexer in `elephc-magician`'s `lexer/inline_html.rs`, so a
//!   physical file and an `eval()` string agree on where code starts.
//! - HTML bytes go through `push_literal_char`, so a private-use marker character
//!   (U+E000–U+E0FF) is escaped rather than emitted as a raw byte, as in the string scanners.

use super::cursor::Cursor;
use super::token::{spanned, SpannedToken, Token};

/// Returns whether `remaining` starts with a `<?php` open tag followed by an ASCII separator
/// or the end of input, case-insensitively.
pub(super) fn at_php_open_tag(remaining: &str) -> bool {
    let bytes = remaining.as_bytes();
    bytes.len() >= 5
        && bytes[..5].eq_ignore_ascii_case(b"<?php")
        && bytes.get(5).copied().is_none_or(is_open_tag_separator)
}

/// Returns whether `remaining` starts with the short echo tag `<?=`.
pub(super) fn at_short_echo_tag(remaining: &str) -> bool {
    remaining.starts_with("<?=")
}

/// Returns whether `byte` may follow `<?php` for the tag to open code.
///
/// The Zend scanner accepts a space, a tab, or a line break, and nothing else — not the other
/// Unicode spaces.
fn is_open_tag_separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// Consumes `?>`, the one newline it swallows, and the inline HTML up to the next open tag.
///
/// Appends the `;` that terminates the statement the close tag ended, then — when the HTML is
/// non-empty — an `echo "…";` statement reproducing it, and an `echo` when the next open tag
/// is the short echo `<?=`. Consumes the opening `<?php`/`<?=` so the caller resumes scanning
/// code.
pub(super) fn scan_close_tag(cursor: &mut Cursor<'_>, out: &mut Vec<SpannedToken>) {
    let close_span = cursor.span();
    cursor.advance(); // '?'
    cursor.advance(); // '>'
    swallow_close_tag_newline(cursor);
    out.push(spanned(Token::Semicolon, close_span));
    scan_inline_html(cursor, out);
}

/// Consumes the inline HTML before the first open tag and appends the `echo` statement that
/// reproduces it, leaving the cursor on the open tag.
///
/// Used when a PHP file starts with HTML (or is entirely HTML); the caller synthesizes the
/// structural `OpenTag` the parser expects before calling this.
pub(super) fn scan_leading_html(cursor: &mut Cursor<'_>, out: &mut Vec<SpannedToken>) {
    scan_inline_html(cursor, out);
}

/// Consumes `\n`, `\r\n` or `\r` immediately after a `?>`, matching PHP's close-tag rule.
fn swallow_close_tag_newline(cursor: &mut Cursor<'_>) {
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

/// Consumes HTML bytes up to the next `<?php`/`<?=` tag (or end of input) and appends the
/// `echo` statement reproducing them.
///
/// The opening tag is consumed so the caller resumes in code; `<?=` additionally appends an
/// `echo` keyword, since `<?= $x ?>` is `echo $x;`.
fn scan_inline_html(cursor: &mut Cursor<'_>, out: &mut Vec<SpannedToken>) {
    let start = cursor.span();
    let mut html = String::new();
    loop {
        let remaining = cursor.remaining();
        if at_short_echo_tag(remaining) || at_php_open_tag(remaining) {
            break;
        }
        match cursor.advance() {
            Some(ch) => crate::string_bytes::push_literal_char(ch, &mut html),
            None => break,
        }
    }
    // The lowered tokens span the HTML text they reproduce. The doc-comment pass walks token
    // extents to find the whitespace/comment gap above a declaration, so a point span here made
    // it read the HTML back as code and drop the `@template` below it. The open tag is consumed
    // below and is not part of the HTML; the gap scan steps over it.
    let html_end = cursor.span();
    let html_span = crate::span::Span::with_end_from(start, html_end);
    if !html.is_empty() {
        out.push(spanned(Token::Echo, html_span));
        out.push(spanned(Token::StringLiteral(html), html_span));
        out.push(spanned(Token::Semicolon, html_span));
    }
    if at_short_echo_tag(cursor.remaining()) {
        let tag_start = cursor.span();
        for _ in 0..3 {
            cursor.advance();
        }
        let echo_span = crate::span::Span::with_end_from(tag_start, cursor.span());
        out.push(spanned(Token::Echo, echo_span));
    } else if at_php_open_tag(cursor.remaining()) {
        for _ in 0..5 {
            cursor.advance();
        }
    }
}
