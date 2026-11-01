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
    // A leading UTF-8 byte-order mark (U+FEFF) is ignored, matching editors that save PHP
    // files as BOM-prefixed UTF-8; stripping it keeps the `<?php` open tag at the start.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    // A leading `#!` line is removed by PHP's CLI SAPI before scanning, in every file it
    // compiles, and kept by a web SAPI, which echoes it like any other text before `<?php`.
    // `first_line` carries the numbering across the removal so diagnostics still name the
    // physical line (`crate::sapi`).
    let (source, first_line) = crate::sapi::strip_leading_shebang(source);
    let mut cursor = Cursor::new_at_line_in_source(source, first_line, source_id);
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
            for _ in 0..3 {
                cursor.advance();
            }
            tokens.push(spanned(Token::OpenTag, span));
            tokens.push(spanned(Token::Echo, span));
        } else if source.contains("<?php") || source.contains("<?=") {
            // Text before the first opening tag is output, exactly like the text after a closing
            // tag. The scan restarts from byte 0 so that text is echoed verbatim, whitespace
            // included — PHP prints those bytes too. A file whose leading bytes are ONLY
            // whitespace never gets here (the skip above already found the tag), which keeps the
            // indented-open-tag spelling behaving as it always has.
            cursor = Cursor::new_at_line_in_source(source, first_line, source_id);
            tokens.push(spanned(Token::OpenTag, cursor.span()));
            scan_inline_html_text(&mut cursor, &mut tokens);
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
        if opens_doc_comment(cursor.remaining()) {
            let mut doc_comment = String::new();
            while let Some(ch) = cursor.advance() {
                doc_comment.push(ch);
                if ch == '*' && cursor.peek() == Some('/') {
                    cursor.advance();
                    doc_comment.push('/');
                    break;
                }
            }
            retain_declaration_doc_comment(&mut tokens, spanned(Token::DocComment(doc_comment), span));
            continue;
        }
        if matches!(mode, SourceMode::Lfc)
            && (cursor.remaining().starts_with("<?php")
                || cursor.remaining().starts_with("?>"))
        {
            return Err(CompileError::new(
                span,
                "PHP opening and closing tags are not valid in .lfc source files",
            ));
        } else if cursor.remaining().starts_with("?>") {
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

    // A docblock that closes a block (`return $x; /** @phpstan-ignore y */ }`) documents nothing:
    // `retain_declaration_doc_comment` keeps it because a `;` precedes it, but no declaration
    // follows, and the statement loops would otherwise parse it as the start of one.
    let mut index = 0;
    while index + 1 < tokens.len() {
        if matches!(tokens[index].0, Token::DocComment(_))
            && matches!(tokens[index + 1].0, Token::RBrace | Token::Eof)
        {
            tokens.remove(index);
        } else {
            index += 1;
        }
    }

    super::qualified_names::identify_reserved_name_segments(&mut tokens);
    Ok(tokens)
}

/// Consumes a PHP closing tag and the literal text that follows it.
///
/// `?>` leaves PHP mode: it terminates the statement it interrupts exactly as `;` does, swallows
/// one immediately following newline, and turns everything up to the next opening tag into output.
/// Emitting that output as `echo '<text>';` keeps the whole feature inside the lexer — the parser,
/// the checker and every backend go on seeing ordinary PHP, and alternative syntax spanning tags
/// (`<?php foreach (…): ?>…<?php endforeach; ?>`) falls out for free.
fn scan_inline_html(cursor: &mut Cursor, tokens: &mut Vec<SpannedToken>) {
    let close_span = cursor.span();
    cursor.advance(); // '?'
    cursor.advance(); // '>'
    // The implicit semicolon is skipped where PHP would only be adding an empty statement: right
    // after a `;`, a block boundary, or the `:` that opens an alternative-syntax body.
    if !matches!(
        tokens.last().map(|(token, _)| token),
        None | Some(
            Token::Semicolon | Token::LBrace | Token::RBrace | Token::Colon | Token::OpenTag
        )
    ) {
        tokens.push(spanned(Token::Semicolon, close_span));
    }
    // PHP eats exactly one newline directly after the closing tag, so a template's line breaks
    // around its tags do not each become a blank line in the output.
    if cursor.remaining().starts_with("\r\n") {
        cursor.advance();
        cursor.advance();
    } else if cursor.peek() == Some('\n') {
        cursor.advance();
    }
    scan_inline_html_text(cursor, tokens);
}

/// Emits the literal text up to the next opening tag as an `echo`, then consumes that tag.
///
/// `<?=` is PHP's short echo tag, so re-entering through it leaves an `echo` for the expression
/// that follows; the `?>` closing it supplies the terminating `;` through [`scan_inline_html`].
fn scan_inline_html_text(cursor: &mut Cursor, tokens: &mut Vec<SpannedToken>) {
    let text_span = cursor.span();
    let mut text = String::new();
    while !cursor.is_eof()
        && !cursor.remaining().starts_with("<?php")
        && !cursor.remaining().starts_with("<?=")
    {
        if let Some(ch) = cursor.advance() {
            text.push(ch);
        }
    }
    if !text.is_empty() {
        tokens.push(spanned(Token::Echo, text_span));
        tokens.push(spanned(Token::StringLiteral(text), text_span));
        tokens.push(spanned(Token::Semicolon, text_span));
    }
    let reopen_span = cursor.span();
    if cursor.remaining().starts_with("<?php") {
        for _ in 0..5 {
            cursor.advance();
        }
    } else if cursor.remaining().starts_with("<?=") {
        for _ in 0..3 {
            cursor.advance();
        }
        tokens.push(spanned(Token::Echo, reopen_span));
    }
}

/// Reports whether `rest` opens a doc comment rather than an ordinary block comment.
///
/// PHP's scanner rule is `"/**"{WHITESPACE}`: the third star must be followed by a space, tab or
/// newline. `/**/`, `/***` banners and `/**x*/` are `T_COMMENT`, so they must not replace the real
/// docblock above them.
fn opens_doc_comment(rest: &str) -> bool {
    rest.strip_prefix("/**")
        .and_then(|after| after.chars().next())
        .is_some_and(|next| matches!(next, ' ' | '\t' | '\n' | '\r'))
}

/// Retains a doc comment only where PHP can begin a declaration statement.
///
/// PHP treats doc comments as whitespace in expression and parameter positions. The parser only
/// consumes them as declaration metadata, so retaining them after an open tag, statement
/// terminator, or block boundary preserves class reflection while avoiding synthetic syntax in
/// ordinary PHP constructs such as `function (/** @var T */ T $value)`.
fn retain_declaration_doc_comment(tokens: &mut Vec<SpannedToken>, doc_comment: SpannedToken) {
    match tokens.last().map(|(token, _)| token) {
        Some(Token::DocComment(_)) => {
            // PHP exposes the nearest contiguous docblock as declaration metadata.
            tokens.pop();
            tokens.push(doc_comment);
        }
        Some(Token::OpenTag | Token::Semicolon | Token::LBrace | Token::RBrace) => {
            tokens.push(doc_comment);
        }
        _ => {}
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

        if opens_doc_comment(cursor.remaining()) {
            break;
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

/// Consumes a `//` or `#` line comment up to the newline that ends it — or to a closing tag,
/// whichever comes first. PHP stops a line comment at `?>` so that `<?php // note ?>` still
/// leaves PHP mode; consuming the tag as comment text would swallow the rest of the template.
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
