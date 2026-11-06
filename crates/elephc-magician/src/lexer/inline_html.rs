//! Purpose:
//! Lexes a `?>` close tag inside eval'd code, and the inline HTML after it, into tokens the
//! grammar already has: the close tag is a `;`, the HTML is `echo "…";`, `<?=` is `echo`.
//!
//! Called from:
//! - `super::scan::Lexer::next_tokens()` when it meets `?>` in code.
//!
//! Key details:
//! - `eval()` source starts in PHP mode, and a `?>` switches to HTML until the next open tag,
//!   exactly as in a file. It used to lex as `?` then `>`, a parse error, so any eval'd code
//!   with a close tag failed: `eval('echo "a"; ?>HTML<?php echo "b";')` printed nothing
//!   where reference PHP 8.5.10 prints `aHTMLb` (MEASURED, with the shapes pinned in
//!   `tests/eval_inline_html_tests.rs`).
//! - Translating to `;` and `echo` keeps a block that spans the tags working
//!   (`if (1) { ?>IN<?php }`), which splitting the source into separate fragments could not.
//! - One newline right after `?>` is swallowed (`\n`, `\r\n` or `\r`), as the Zend scanner's
//!   close-tag rule does.
//! - `<?php` opens code only when followed by a space, a tab, a line break, or the end of
//!   input (`<?phpX` and `<?php` + U+00A0 are HTML), case-insensitively — the include
//!   segmenter's rule, shared through `script_cache::segments::is_open_tag_separator`. The
//!   short `<?` tag is HTML: `short_open_tag` is off in the production php.ini, and the
//!   include segmenter does not open on it either.

use super::scan::Lexer;
use crate::script_cache::segments::is_open_tag_separator;
use super::{Token, TokenKind};
use elephc_builtin_contract::string_literal::push_literal_char;

impl Lexer<'_> {
    /// Consumes `?>`, the newline it swallows, and the inline HTML up to the next open tag.
    pub(super) fn lex_close_tag(&mut self, line: i64) -> Vec<Token> {
        self.bump_char();
        self.bump_char();
        match (self.peek_char(), self.peek_next_char()) {
            (Some('\r'), Some('\n')) => {
                self.bump_char();
                self.bump_char();
            }
            (Some('\n' | '\r'), _) => self.bump_char(),
            _ => {}
        }

        let mut tokens = vec![Token::new(TokenKind::Semicolon, line)];
        let mut html = String::new();
        let mut short_echo = false;
        while let Some(ch) = self.peek_char() {
            if ch == '<' && self.peek_next_char() == Some('?') {
                if self.at_php_open_tag() {
                    for _ in 0.."<?php".len() {
                        self.bump_char();
                    }
                    break;
                }
                if self.peek_nth_char(2) == Some('=') {
                    for _ in 0.."<?=".len() {
                        self.bump_char();
                    }
                    short_echo = true;
                    break;
                }
            }
            push_literal_char(ch, &mut html);
            self.bump_char();
        }
        if !html.is_empty() {
            tokens.push(Token::new(TokenKind::Ident("echo".to_string()), line));
            tokens.push(Token::new(TokenKind::String(html), line));
            tokens.push(Token::new(TokenKind::Semicolon, line));
        }
        if short_echo {
            tokens.push(Token::new(TokenKind::Ident("echo".to_string()), line));
        }
        tokens
    }

    /// Returns whether the cursor is at `<?php` followed by an ASCII separator or the end of
    /// input.
    fn at_php_open_tag(&self) -> bool {
        let tag: String = (0..5).filter_map(|offset| self.peek_nth_char(offset)).collect();
        tag.eq_ignore_ascii_case("<?php")
            && self.peek_nth_char(5).is_none_or(|next| {
                next.is_ascii() && is_open_tag_separator(next as u8)
            })
    }
}
