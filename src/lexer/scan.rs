//! Purpose:
//! Owns the main PHP token scanning loop and dispatches literal-specific scanners.
//! Skips whitespace/comments and emits structural, operator, keyword, and literal tokens.
//!
//! Called from:
//! - `crate::lexer::tokenize()`.
//!
//! Key details:
//! - Multi-character operators and PHP opening tags must be recognized before shorter prefixes.

use super::cursor::Cursor;
use super::literals;
use super::token::{spanned, SpannedToken, Token, TokenMetadata};
use crate::errors::CompileError;
use crate::source::SourceMode;

/// Scans the full PHP source into a stream of syntax tokens with source metadata.
///
/// Requires `<?php` as the first five characters. Dispatches to `literals` for
/// strings (which may contain interpolation), heredoc/nowdoc, numbers, variables,
/// and keywords. Returns `Token::Eof` at end-of-input.
///
/// # Errors
/// Returns `CompileError` when PHP mode lacks its opening tag, LFC contains a
/// physical PHP tag at a code boundary, or either mode contains invalid syntax.
///
/// A `?>` closing tag is handled by [`scan_inline_html`], which turns the literal text that
/// follows into an ordinary `echo` of a string constant.
pub fn scan_tokens(
    source: &str,
    mode: SourceMode,
) -> Result<Vec<SpannedToken>, CompileError> {
    // A leading UTF-8 byte-order mark (U+FEFF) is ignored, matching editors that save PHP
    // files as BOM-prefixed UTF-8; stripping it keeps the `<?php` open tag at the start.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut cursor = Cursor::new(source);
    let mut tokens = Vec::new();

    let span = cursor.span();
    if mode.requires_open_tag() {
        skip_whitespace_and_comments(&mut cursor);
        let span = cursor.span();
        if cursor.remaining().starts_with("<?php") {
            for _ in 0..5 {
                cursor.advance();
            }
            tokens.push(spanned(Token::OpenTag, span));
        } else if cursor.remaining().starts_with("<?=") {
            // php's short echo opens a file as readily as `<?php` does.
            for _ in 0..3 {
                cursor.advance();
            }
            tokens.push(spanned(Token::OpenTag, span));
            tokens.push(spanned(Token::Echo, cursor.span()));
        } else {
            return Err(CompileError::new(span, "Expected '<?php' at start of file"));
        }
    } else {
        tokens.push(spanned(Token::OpenTag, span));
    }

    loop {
        skip_whitespace_and_comments(&mut cursor);

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
            // A closing tag ENDS the statement and hands the rest of the file to the output.
            //
            // php treats `?>` as an implicit `;`, swallows ONE newline directly after it, and
            // echoes everything up to the next `<?php` verbatim. Measured on `php -n` 8.5.6:
            // `<?php echo "A";?>\nX\n<?php echo "B";` prints `AX\nB` — the newline after the tag
            // is gone, the one after `X` is not.
            //
            // The literal text becomes `echo <string>;` at the TOKEN level, so nothing downstream
            // needs to learn about inline HTML: the parser, the checker and codegen all see an
            // ordinary echo of a string constant.
            scan_inline_html(&mut cursor, &mut tokens);
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
            let span = crate::span::Span::with_end(span.line, span.col, end.line, end.col);
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



/// Skips a `//` or `#` line comment, which php ends at a newline OR at a CLOSING TAG.
///
/// `<?php echo "A"; // comment ?>TEXT` prints `ATEXT`: the `?>` inside the comment closes the tag
/// and hands `TEXT` to the output. Running to the newline instead swallowed the tag, and the
/// `<?php` on the next line then arrived as code — a parse error on a file php accepts. Measured on
/// `php -n` 8.5.6, for both comment introducers.
///
/// A `/* */` block comment does NOT stop there: `/* block ?> still comment */ echo "B";` prints
/// `AB`, so the tag inside it is ordinary comment text and only `*/` ends it.
///
/// The tag itself is left UNCONSUMED, so the scan loop sees it and takes the inline-HTML path.
fn skip_line_comment(cursor: &mut Cursor) {
    while let Some(ch) = cursor.peek() {
        if cursor.remaining().starts_with("?>") {
            return;
        }
        cursor.advance();
        if ch == '\n' {
            return;
        }
    }
}

/// Consumes a `?>` closing tag and the literal text after it, emitting `; echo <text>;`.
///
/// php's rules here are three, and each is measured on `php -n` 8.5.6:
///
/// - `?>` terminates the current statement, so a trailing `;` is optional before it.
/// - ONE newline directly after the tag is swallowed. `<?php echo "A";?>\nX\n<?php echo "B";`
///   prints `AX\nB`: the newline after the tag is gone, the one after `X` is not. A `\r\n` counts
///   as that one newline.
/// - Everything up to the next `<?php` — or to end of file — is output verbatim, whatever it
///   contains. It is not PHP and is never scanned as PHP.
///
/// Emitting `echo` + a string literal + `;` keeps the whole feature inside the lexer: the parser
/// sees an ordinary echo statement, so nothing downstream needs an inline-HTML node. Empty text
/// emits nothing at all, which is what `?><?php` should cost.
fn scan_inline_html(cursor: &mut Cursor, tokens: &mut Vec<SpannedToken>) {
    let tag_span = cursor.span();
    cursor.advance();                                                   // '?'
    cursor.advance();                                                   // '>'
    // The closing tag stands in for the statement's semicolon — but only where one belongs. php
    // accepts the empty statement a doubled `;` makes; this parser has no such production, so
    // `<?php echo "A"; ?>` would fail on the spare token. Nor does a `;` belong after the tokens
    // that OPEN something: `<?php ?>`, a `{` or `}` around a block, or the `:` of the alternative
    // `if (...): ?>` form, which php uses precisely to wrap literal text.
    if !matches!(
        tokens.last().map(|(token, _)| token),
        Some(Token::Semicolon | Token::LBrace | Token::RBrace | Token::OpenTag | Token::Colon)
    ) {
        tokens.push(spanned(Token::Semicolon, tag_span));
    }

    if cursor.remaining().starts_with("\r\n") {
        cursor.advance();
        cursor.advance();
    } else if cursor.peek() == Some('\n') {
        cursor.advance();
    }

    let text_span = cursor.span();
    let mut text = String::new();
    let mut short_echo = false;
    while !cursor.is_eof() {
        if cursor.remaining().starts_with("<?php") {
            for _ in 0..5 {
                cursor.advance();
            }
            break;
        }
        // `<?=` is an opening tag too — php's short echo, exactly `<?php echo`. It has to be
        // recognised HERE or the text swallows it: measured on `php -n` 8.5.6,
        // `Hello <?= $name ?>!` prints the value, and a `<?=` inside what looked like literal
        // text is a parse error, not literal output.
        if cursor.remaining().starts_with("<?=") {
            for _ in 0..3 {
                cursor.advance();
            }
            short_echo = true;
            break;
        }
        if let Some(ch) = cursor.advance() {
            text.push(ch);
        }
    }

    if !text.is_empty() {
        tokens.push(spanned(Token::Echo, text_span));
        tokens.push(spanned(Token::StringLiteral(text), text_span));
        tokens.push(spanned(Token::Semicolon, text_span));
    }
    if short_echo {
        // The expression that follows is scanned as ordinary PHP; its `?>` supplies the `;`,
        // and an explicit one is equally accepted.
        tokens.push(spanned(Token::Echo, cursor.span()));
    }
}

/// Skips all whitespace, `//` line comments, `#` line comments (but not `#[` attribute
/// groups), and `/* */` block comments. Uses `continue` to re-check after each comment
/// type so adjacent comment forms are all skipped.
fn skip_whitespace_and_comments(cursor: &mut Cursor) {
    loop {
        while let Some(ch) = cursor.peek() {
            if ch.is_ascii_whitespace() {
                cursor.advance();
            } else {
                break;
            }
        }

        if cursor.remaining().starts_with("//") {
            skip_line_comment(cursor);
            continue;
        }

        if cursor.remaining().starts_with('#') && !cursor.remaining().starts_with("#[") {
            // PHP line comment introduced by `#` (but `#[` opens an attribute group).
            skip_line_comment(cursor);
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
