//! Purpose:
//! Owns the main PHP token scanning loop and dispatches literal-specific scanners.
//! Skips whitespace/comments and emits structural, operator, keyword, and literal tokens.
//!
//! Called from:
//! - `crate::lexer::tokenize()`.
//!
//! Key details:
//! - Multi-character operators and PHP opening tags must be recognized before shorter prefixes.
//! - Keywords touched by `\` inside a qualified name are emitted as identifiers (PHP 8 T_NAME_QUALIFIED).

use super::cursor::Cursor;
use super::inline_html;
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
    let mut cursor = Cursor::new_in_source(source, source_id);
    let mut tokens = Vec::new();

    let span = cursor.span();
    // The parser's structural boundary token: a PHP file starts in HTML mode, so it is
    // synthesized at the start and any leading text follows as `echo "text";`.
    tokens.push(spanned(Token::OpenTag, span));
    if mode.requires_open_tag() {
        // PHP CLI skips a leading shebang line.
        if cursor.remaining().starts_with("#!") {
            while let Some(ch) = cursor.advance() {
                if ch == '\n' {
                    break;
                }
            }
        }
        inline_html::scan_inline_html(&mut cursor, &mut tokens);
    }

    // `cursor.remaining().len()` right after a `\` token, so the next word can tell whether
    // the separator touches it (PHP 8 lexes `A\Default\X` as one T_NAME_QUALIFIED token).
    let mut backslash_end: Option<usize> = None;
    // Whether that `\` starts the name (no word touches it from the left): a sole
    // `\self` / `\parent` / `\static` segment must keep its keyword token.
    let mut backslash_leads_name = false;
    // `cursor.remaining().len()` right after a word token, to classify the next `\`.
    let mut word_end: Option<usize> = None;
    loop {
        skip_whitespace_and_comments(&mut cursor);

        if cursor.is_eof() {
            tokens.push(spanned(Token::Eof, cursor.span()));
            break;
        }

        let span = cursor.span();
        if mode.requires_open_tag() && cursor.remaining().starts_with("?>") {
            // `?>` is an implicit `;`; the text after it is inline HTML up to the next tag.
            tokens.push(spanned(Token::Semicolon, span));
            inline_html::consume_close_tag(&mut cursor);
            inline_html::scan_inline_html(&mut cursor, &mut tokens);
            backslash_end = None;
            word_end = None;
        } else if matches!(mode, SourceMode::Lfc)
            && (cursor.remaining().starts_with("<?php")
                || cursor.remaining().starts_with("?>"))
        {
            return Err(CompileError::new(
                span,
                "PHP opening and closing tags are not valid in .lfc source files",
            ));
        } else if cursor.peek() == Some('"') {
            // Double-quoted strings may contain interpolation ($var)
            let string_tokens = literals::scan_double_string_interpolated(&mut cursor)?;
            tokens.extend(string_tokens);
            backslash_end = None;
            word_end = None;
        } else if cursor.remaining().starts_with("<<<") {
            // Heredoc/nowdoc — may contain interpolation ($var) for heredoc
            cursor.advance(); // consume first <
            cursor.advance(); // consume second <
            cursor.advance(); // consume third <
            let heredoc_tokens = literals::scan_heredoc(&mut cursor)?;
            tokens.extend(heredoc_tokens);
            backslash_end = None;
            word_end = None;
        } else {
            let starts_word = cursor.peek().is_some_and(literals::is_ident_start);
            let remaining_before = cursor.remaining();
            let touches_backslash = backslash_end == Some(remaining_before.len());
            let mut token = scan_token(&mut cursor)?;
            let end = cursor.span();
            let span = crate::span::Span::with_end_from(span, end);
            let consumed_len = remaining_before.len() - cursor.remaining().len();
            if starts_word
                && keyword_is_name_segment(
                    &token,
                    touches_backslash,
                    touches_backslash && backslash_leads_name,
                    cursor.remaining(),
                )
            {
                token = Token::Identifier(remaining_before[..consumed_len].to_string());
            }
            if matches!(token, Token::Backslash) {
                backslash_leads_name = word_end != Some(remaining_before.len());
                backslash_end = Some(cursor.remaining().len());
            } else {
                backslash_end = None;
            }
            word_end = starts_word.then(|| cursor.remaining().len());
            let metadata = if starts_word && !matches!(token, Token::Identifier(_)) {
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

/// Returns whether a just-scanned keyword token is really a segment of a PHP 8 qualified name.
///
/// PHP 8 lexes `A\Default\X` and `\Default\X` as single T_NAME_QUALIFIED /
/// T_NAME_FULLY_QUALIFIED tokens, so any reserved word is a plain segment when a `\` touches
/// it inside the name: either a separator immediately before it, or a separator immediately
/// after it that is itself followed by a name character. A lone keyword (`new Default`,
/// `use A\{Default}`, `Default\ X`) stays reserved.
///
/// Exceptions kept as their dedicated tokens:
/// - `namespace\X` as the first segment is PHP's T_NAME_RELATIVE, not a name segment.
/// - `\self`, `\parent` and `\static` as the only segment keep their keyword token: PHP
///   resolves `\static::class` like `static::class` and rejects every other use with
///   "'\static' is an invalid class name", which the parser reports.
/// - A final segment after `\` that is a literal, predefined, or magic constant (`\true`,
///   `\PHP_EOL`, `A\PHP_EOL`) keeps its token; the parser already accepts those spellings.
fn keyword_is_name_segment(
    token: &Token,
    touches_backslash: bool,
    after_leading_backslash: bool,
    rest: &str,
) -> bool {
    if matches!(token, Token::Identifier(_)) || token.canonical_word_spelling().is_none() {
        return false;
    }
    let mut after = rest.chars();
    let followed_by_segment =
        after.next() == Some('\\') && after.next().is_some_and(literals::is_ident_start);
    if followed_by_segment {
        return touches_backslash || !matches!(token, Token::Namespace);
    }
    if after_leading_backslash && matches!(token, Token::Self_ | Token::Parent | Token::Static) {
        return false;
    }
    touches_backslash && !is_constant_word_token(token)
}

/// Returns whether a word token denotes a literal, predefined, or magic constant.
fn is_constant_word_token(token: &Token) -> bool {
    matches!(
        token,
        Token::True
            | Token::False
            | Token::Null
            | Token::Inf
            | Token::Nan
            | Token::PhpIntMax
            | Token::PhpIntMin
            | Token::PhpFloatMax
            | Token::PhpFloatMin
            | Token::PhpFloatEpsilon
            | Token::MPi
            | Token::ME
            | Token::MSqrt2
            | Token::MPi2
            | Token::MPi4
            | Token::MLog2e
            | Token::MLog10e
            | Token::Stdin
            | Token::Stdout
            | Token::Stderr
            | Token::PhpEol
            | Token::PhpOs
            | Token::DirectorySeparator
            | Token::DunderDir
            | Token::DunderFile
            | Token::DunderLine
            | Token::DunderFunction
            | Token::DunderClass
            | Token::DunderMethod
            | Token::DunderNamespace
            | Token::DunderTrait
    )
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

/// Skips a `//` or `#` line comment: through its newline, or up to (not including) a `?>`, which
/// ends a PHP line comment and closes the PHP block.
fn skip_line_comment(cursor: &mut Cursor) {
    while !cursor.remaining().starts_with("?>") {
        match cursor.advance() {
            Some('\n') | None => break,
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
