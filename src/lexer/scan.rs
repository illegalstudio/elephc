use super::cursor::Cursor;
use super::literals;
use super::token::Token;
use crate::errors::CompileError;
use crate::span::Span;

pub fn scan_tokens(source: &str) -> Result<Vec<(Token, Span)>, CompileError> {
    let mut cursor = Cursor::new(source);
    let mut tokens = Vec::new();

    skip_whitespace_and_comments(&mut cursor);

    let span = cursor.span();
    if cursor.remaining().starts_with("<?php") {
        for _ in 0..5 {
            cursor.advance();
        }
        tokens.push((Token::OpenTag, span));
    } else {
        return Err(CompileError::new(span, "Expected '<?php' at start of file"));
    }

    loop {
        skip_whitespace_and_comments(&mut cursor);

        if cursor.is_eof() {
            tokens.push((Token::Eof, cursor.span()));
            break;
        }

        let span = cursor.span();
        if cursor.peek() == Some('"') {
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
            let token = scan_token(&mut cursor)?;
            tokens.push((token, span));
        }
    }

    Ok(tokens)
}

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
            while let Some(ch) = cursor.advance() {
                if ch == '\n' { break; }
            }
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
            cursor.advance();
            if cursor.peek() == Some('?') {
                cursor.advance();
                if cursor.peek() == Some('=') {
                    cursor.advance();
                    Ok(Token::QuestionQuestionAssign)
                } else {
                    Ok(Token::QuestionQuestion)
                }
            } else {
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
        '$' => literals::scan_variable(cursor),
        '0'..='9' => literals::scan_number(cursor),
        'a'..='z' | 'A'..='Z' | '_' => literals::scan_keyword(cursor),
        _ => Err(CompileError::new(
            cursor.span(),
            &format!("Unexpected character: '{}'", ch),
        )),
    }
}
