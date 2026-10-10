//! Purpose:
//! Owns the main PHP token scanning loop and dispatches literal-specific scanners.
//! Skips whitespace/comments, handles PHP/HTML tag boundaries, and emits structural,
//! operator, keyword, and literal tokens.
//!
//! Called from:
//! - `crate::lexer::tokenize()`.
//!
//! Key details:
//! - Multi-character operators and PHP opening tags must be recognized before shorter prefixes.
//! - `?>` and the inline HTML after it are lowered by `super::inline_html`; a one-line comment
//!   ends at `?>` as well as at the end of the line, as PHP's scanner does.

use super::cursor::Cursor;
use super::inline_html;
use super::literals;
use super::token::{spanned, SpannedToken, Token, TokenMetadata};
use crate::errors::CompileError;
use crate::source::SourceMode;

/// Scans the full PHP source into a stream of syntax tokens with source metadata.
///
/// PHP source may begin with inline HTML before the first `<?php`/`<?=` open tag and may
/// interleave HTML through `?>` close tags; each HTML run is lowered to an `echo` statement.
/// Dispatches to `literals` for strings (which may contain interpolation), heredoc/nowdoc,
/// numbers, variables, and keywords. Returns `Token::Eof` at end-of-input.
///
/// # Errors
/// Returns `CompileError` when LFC contains a physical PHP tag at a code boundary, or either
/// mode contains invalid syntax.
pub fn scan_tokens(
    source: &str,
    mode: SourceMode,
) -> Result<Vec<SpannedToken>, CompileError> {
    scan_tokens_in_source(source, mode, 0)
}

/// Tokenizes one physical source while assigning a disambiguating source identity to its spans.
pub(super) fn scan_tokens_in_source(
    source: &str,
    mode: SourceMode,
    source_id: u32,
) -> Result<Vec<SpannedToken>, CompileError> {
    // PHP recognizes a shebang only when the file's RAW first two bytes are `#!`; a leading BOM
    // (or any other byte) makes the line ordinary inline HTML, which is emitted.
    let starts_with_shebang = source.starts_with("#!");

    // A leading UTF-8 byte-order mark (U+FEFF) is ignored, matching editors that save PHP
    // files as BOM-prefixed UTF-8; stripping it keeps the `<?php` open tag at the start.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut cursor = Cursor::new_in_source(source, source_id);
    let mut tokens = Vec::new();

    // PHP drops a shebang only when the file starts with `#!`, through the following `\n` or the
    // end of input; a later `#!` is ordinary text. LFC mode treats `#!` as an ordinary `#`
    // comment, which the scanner already skips.
    if mode.requires_open_tag() && starts_with_shebang {
        skip_shebang(&mut cursor);
    }

    let span = cursor.span();
    if mode.requires_open_tag() {
        // A PHP file may open with inline HTML (or be entirely HTML), with `<?php`/`<?=`
        // appearing later. The structural `OpenTag` is always emitted first because the parser
        // requires it, and leading HTML becomes an `echo` ahead of the first statement.
        tokens.push(spanned(Token::OpenTag, span));
        if inline_html::at_php_open_tag(cursor.remaining()) {
            for _ in 0..5 {
                cursor.advance();
            }
        } else if inline_html::at_short_echo_tag(cursor.remaining()) {
            for _ in 0..3 {
                cursor.advance();
            }
            tokens.push(spanned(Token::Echo, span));
        } else {
            inline_html::scan_leading_html(&mut cursor, &mut tokens);
        }
    } else {
        tokens.push(spanned(Token::OpenTag, span));
    }

    loop {
        // In PHP mode a line comment ends at a `?>` close tag; in LFC mode there is no HTML
        // mode, so tag-shaped text in a comment stays ordinary data.
        skip_whitespace_and_comments(&mut cursor, mode.requires_open_tag());

        if cursor.is_eof() {
            tokens.push(spanned(Token::Eof, cursor.span()));
            break;
        }

        let span = cursor.span();
        if matches!(mode, SourceMode::Lfc)
            && (cursor.remaining().starts_with("<?php")
                || cursor.remaining().starts_with("?>"))
        {
            return Err(CompileError::new(
                span,
                "PHP opening and closing tags are not valid in .lfc source files",
            ));
        } else if cursor.remaining().starts_with("?>") {
            // A close tag ends the current statement and switches to inline HTML until the
            // next open tag: the close tag is a `;` and the HTML is an `echo`, exactly as in a
            // file. In LFC mode the branch above already rejected it.
            inline_html::scan_close_tag(&mut cursor, &mut tokens);
        } else if cursor.peek() == Some('"') {
            // Double-quoted strings may contain interpolation ($var)
            let string_tokens = literals::scan_double_string_interpolated(&mut cursor)?;
            tokens.extend(string_tokens);
        } else if cursor.remaining().starts_with("<<<") {
            // Heredoc/nowdoc — may contain interpolation ($var) for heredoc
            cursor.advance(); // consume first <
            cursor.advance(); // consume second <
            cursor.advance(); // consume third <
            let heredoc_tokens = literals::scan_heredoc(&mut cursor)?;
            tokens.extend(heredoc_tokens);
        } else {
            let starts_word = cursor.peek().is_some_and(literals::is_ident_start);
            let remaining_before = cursor.remaining();
            let token = scan_token(&mut cursor)?;
            let end = cursor.span();
            let span = crate::span::Span::with_end_from(span, end);
            let metadata = if starts_word && !matches!(token, Token::Identifier(_)) {
                let consumed_len = remaining_before.len() - cursor.remaining().len();
                let source_spelling = &remaining_before[..consumed_len];
                if token.canonical_word_spelling() == Some(source_spelling) {
                    TokenMetadata::new(span)
                } else {
                    TokenMetadata::with_source_spelling(span, source_spelling)
                }
            } else {
                TokenMetadata::new(span)
            };
            tokens.push((token, metadata));
        }
    }

    Ok(tokens)
}

/// Skips all whitespace, `//` line comments, `#` line comments (but not `#[` attribute
/// groups), and `/* */` block comments. Uses `continue` to re-check after each comment
/// type so adjacent comment forms are all skipped.
///
/// `stop_at_close_tag` is true in PHP mode, where a one-line comment ends at a `?>` as well
/// as at the end of the line; LFC mode passes false so tag-shaped comment text stays data.
fn skip_whitespace_and_comments(cursor: &mut Cursor, stop_at_close_tag: bool) {
    loop {
        while let Some(ch) = cursor.peek() {
            if ch.is_ascii_whitespace() {
                cursor.advance();
            } else {
                break;
            }
        }

        if cursor.remaining().starts_with("//") {
            skip_line_comment(cursor, stop_at_close_tag);
            continue;
        }

        if cursor.remaining().starts_with('#') && !cursor.remaining().starts_with("#[") {
            // PHP line comment introduced by `#` (but `#[` opens an attribute group).
            skip_line_comment(cursor, stop_at_close_tag);
            continue;
        }

        if cursor.remaining().starts_with("/*") {
            cursor.advance();
            cursor.advance();
            loop {
                match cursor.advance() {
                    Some('*') if cursor.peek() == Some('/') => {
                        cursor.advance();
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            continue;
        }

        break;
    }
}

/// Consumes a file-initial `#!` shebang line, through its terminating `\n` or the end of input.
///
/// PHP drops the first line of a file only when it begins with `#!`; the `#!` is then not part
/// of the output. It ends only at `\n`: a bare `\r` is shebang content, and a shebang with no
/// `\n` at all consumes the whole file. A `#!` anywhere else is ordinary inline HTML.
fn skip_shebang(cursor: &mut Cursor) {
    loop {
        match cursor.advance() {
            Some('\n') | None => return,
            Some(_) => {}
        }
    }
}

/// Advances past one `//` or `#` line comment, including its trailing newline.
///
/// When `stop_at_close_tag` is set (PHP mode), the comment ends at the end of the line OR at a
/// `?>` close tag, whichever comes first, as PHP's rule says: `// note ?>HTML` prints `HTML`,
/// so the close tag is left for the main loop to turn into a statement terminator and the HTML
/// that follows. In LFC mode it is false, so a `?>` inside a comment stays comment text.
fn skip_line_comment(cursor: &mut Cursor, stop_at_close_tag: bool) {
    loop {
        if cursor.is_eof() || (stop_at_close_tag && cursor.remaining().starts_with("?>")) {
            return;
        }
        match cursor.advance() {
            Some('\n' | '\r') | None => return,
            Some(_) => {}
        }
    }
}

/// Dispatches token scanning based on the current character.
///
/// Multi-character operators (`?->`, `??`, `??=`, `:`, `::`, `=>`, `<=>`, `->>`,
/// `<<`, `>>`, `...`, compound assignments, etc.) are recognized before returning.
/// Delegates to `literals` for single-quoted strings, double-quoted strings (in the
/// outer loop), heredoc/nowdoc (in the outer loop), numbers, variables, and keywords.
/// Returns `Token::Eof` when `cursor.peek()` is `None`.
fn scan_token(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let ch = match cursor.peek() {
        Some(c) => c,
        None => return Ok(Token::Eof),
    };

    match ch {
        ';' => { cursor.advance(); Ok(Token::Semicolon) }
        ',' => { cursor.advance(); Ok(Token::Comma) }
        '\\' => { cursor.advance(); Ok(Token::Backslash) }
        '?' => {
            if cursor.remaining().starts_with("?->") {
                cursor.advance();
                cursor.advance();
                cursor.advance();
                Ok(Token::QuestionArrow)
            } else if cursor.remaining().starts_with("??") {
                cursor.advance();
                cursor.advance();
                if cursor.peek() == Some('=') {
                    cursor.advance();
                    Ok(Token::QuestionQuestionAssign)
                } else {
                    Ok(Token::QuestionQuestion)
                }
            } else {
                cursor.advance();
                Ok(Token::Question)
            }
        }
        ':' => {
            cursor.advance();
            if cursor.peek() == Some(':') { cursor.advance(); Ok(Token::DoubleColon) }
            else { Ok(Token::Colon) }
        }
        '(' => { cursor.advance(); Ok(Token::LParen) }
        ')' => { cursor.advance(); Ok(Token::RParen) }
        '{' => { cursor.advance(); Ok(Token::LBrace) }
        '}' => { cursor.advance(); Ok(Token::RBrace) }
        '[' => { cursor.advance(); Ok(Token::LBracket) }
        ']' => { cursor.advance(); Ok(Token::RBracket) }
        '=' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::EqualEqualEqual) }
                else { Ok(Token::EqualEqual) }
            }
            else if cursor.peek() == Some('>') { cursor.advance(); Ok(Token::DoubleArrow) }
            else { Ok(Token::Assign) }
        }
        '!' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::NotEqualEqual) }
                else { Ok(Token::NotEqual) }
            }
            else { Ok(Token::Bang) }
        }
        '&' => {
            cursor.advance();
            if cursor.peek() == Some('&') { cursor.advance(); Ok(Token::AndAnd) }
            else if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::AmpAssign) }
            else { Ok(Token::Ampersand) }
        }
        '|' => {
            cursor.advance();
            if cursor.peek() == Some('|') { cursor.advance(); Ok(Token::OrOr) }
            else if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::PipeAssign) }
            else if cursor.peek() == Some('>') { cursor.advance(); Ok(Token::PipeArrow) }
            else { Ok(Token::Pipe) }
        }
        '^' => {
            cursor.advance();
            if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::CaretAssign) }
            else { Ok(Token::Caret) }
        }
        '~' => { cursor.advance(); Ok(Token::Tilde) }
        '<' => {
            cursor.advance();
            if cursor.peek() == Some('<') {
                cursor.advance();
                if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::LessLessAssign) }
                else { Ok(Token::LessLess) }
            }
            else if cursor.peek() == Some('=') {
                cursor.advance();
                if cursor.peek() == Some('>') { cursor.advance(); Ok(Token::Spaceship) }
                else { Ok(Token::LessEqual) }
            }
            else if cursor.peek() == Some('>') { cursor.advance(); Ok(Token::LessGreater) }
            else { Ok(Token::Less) }
        }
        '>' => {
            cursor.advance();
            if cursor.peek() == Some('>') {
                cursor.advance();
                if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::GreaterGreaterAssign) }
                else { Ok(Token::GreaterGreater) }
            }
            else if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::GreaterEqual) }
            else { Ok(Token::Greater) }
        }
        '+' => {
            cursor.advance();
            match cursor.peek() {
                Some('+') => { cursor.advance(); Ok(Token::PlusPlus) }
                Some('=') => { cursor.advance(); Ok(Token::PlusAssign) }
                _ => Ok(Token::Plus),
            }
        }
        '-' => {
            cursor.advance();
            match cursor.peek() {
                Some('>') => { cursor.advance(); Ok(Token::Arrow) }
                Some('-') => { cursor.advance(); Ok(Token::MinusMinus) }
                Some('=') => { cursor.advance(); Ok(Token::MinusAssign) }
                _ => Ok(Token::Minus),
            }
        }
        '*' => {
            cursor.advance();
            match cursor.peek() {
                Some('*') => {
                    cursor.advance();
                    if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::StarStarAssign) }
                    else { Ok(Token::StarStar) }
                }
                Some('=') => { cursor.advance(); Ok(Token::StarAssign) }
                _ => Ok(Token::Star),
            }
        }
        '/' => {
            cursor.advance();
            if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::SlashAssign) }
            else { Ok(Token::Slash) }
        }
        '%' => {
            cursor.advance();
            if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::PercentAssign) }
            else { Ok(Token::Percent) }
        }
        '.' => {
            // Check if next char is a digit → float literal like .5
            let remaining = cursor.remaining();
            if remaining.len() > 1 && remaining.as_bytes()[1].is_ascii_digit() {
                return literals::scan_dot_float(cursor);
            }
            // Check for ... (ellipsis / spread operator)
            if remaining.starts_with("...") {
                cursor.advance(); // consume first .
                cursor.advance(); // consume second .
                cursor.advance(); // consume third .
                return Ok(Token::Ellipsis);
            }
            cursor.advance();
            if cursor.peek() == Some('=') { cursor.advance(); Ok(Token::DotAssign) }
            else { Ok(Token::Dot) }
        }
        // '"' is handled in the main loop (interpolation support)
        '\'' => literals::scan_single_string(cursor),
        '@' => { cursor.advance(); Ok(Token::At) }
        '#' => {
            if cursor.remaining().starts_with("#[") {
                cursor.advance(); // consume '#'
                cursor.advance(); // consume '['
                Ok(Token::AttrOpen)
            } else {
                // Bare '#' that wasn't consumed by skip_whitespace_and_comments
                Err(CompileError::new(cursor.span(), "Unexpected '#'"))
            }
        }
        '$' => literals::scan_variable(cursor),
        '0'..='9' => literals::scan_number(cursor),
        'a'..='z' | 'A'..='Z' | '_' => literals::scan_keyword(cursor),
        // PHP allows non-ASCII identifier characters (bytes 0x80-0xFF), so a word that
        // starts with one is scanned as an identifier rather than rejected.
        c if literals::is_ident_start(c) => literals::scan_keyword(cursor),
        _ => Err(CompileError::new(
            cursor.span(),
            &format!("Unexpected character: '{}'", ch),
        )),
    }
}
