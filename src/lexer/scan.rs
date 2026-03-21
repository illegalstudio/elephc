use super::cursor::Cursor;
use super::token::Token;
use crate::errors::CompileError;
use crate::span::Span;

pub fn scan_tokens(source: &str) -> Result<Vec<(Token, Span)>, CompileError> {
    let mut cursor = Cursor::new(source);
    let mut tokens = Vec::new();

    skip_whitespace(&mut cursor);

    // Expect <?php open tag
    let span = cursor.span();
    if cursor.remaining().starts_with("<?php") {
        for _ in 0..5 {
            cursor.advance();
        }
        tokens.push((Token::OpenTag, span));
    } else {
        return Err(CompileError::new(
            span,
            "Expected '<?php' at start of file",
        ));
    }

    loop {
        skip_whitespace(&mut cursor);
        skip_comments(&mut cursor);
        skip_whitespace(&mut cursor);

        if cursor.is_eof() {
            tokens.push((Token::Eof, cursor.span()));
            break;
        }

        let span = cursor.span();
        let token = scan_token(&mut cursor)?;
        tokens.push((token, span));
    }

    Ok(tokens)
}

fn skip_whitespace(cursor: &mut Cursor) {
    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_whitespace() {
            cursor.advance();
        } else {
            break;
        }
    }
}

fn skip_comments(cursor: &mut Cursor) {
    if cursor.remaining().starts_with("//") {
        while let Some(ch) = cursor.advance() {
            if ch == '\n' {
                break;
            }
        }
    } else if cursor.remaining().starts_with("/*") {
        cursor.advance(); // /
        cursor.advance(); // *
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
    }
}

fn scan_token(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let ch = cursor.peek().unwrap();

    match ch {
        ';' => {
            cursor.advance();
            Ok(Token::Semicolon)
        }
        '(' => {
            cursor.advance();
            Ok(Token::LParen)
        }
        ')' => {
            cursor.advance();
            Ok(Token::RParen)
        }
        '{' => {
            cursor.advance();
            Ok(Token::LBrace)
        }
        '}' => {
            cursor.advance();
            Ok(Token::RBrace)
        }
        '=' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::EqualEqual)
            } else {
                Ok(Token::Assign)
            }
        }
        '!' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::NotEqual)
            } else {
                Err(CompileError::new(cursor.span(), "Expected '=' after '!'"))
            }
        }
        '<' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::LessEqual)
            } else {
                Ok(Token::Less)
            }
        }
        '>' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::GreaterEqual)
            } else {
                Ok(Token::Greater)
            }
        }
        '+' => {
            cursor.advance();
            if cursor.peek() == Some('+') {
                cursor.advance();
                Ok(Token::PlusPlus)
            } else {
                Ok(Token::Plus)
            }
        }
        '-' => {
            cursor.advance();
            if cursor.peek() == Some('-') {
                cursor.advance();
                Ok(Token::MinusMinus)
            } else {
                Ok(Token::Minus)
            }
        }
        '*' => {
            cursor.advance();
            Ok(Token::Star)
        }
        '/' => {
            cursor.advance();
            Ok(Token::Slash)
        }
        '%' => {
            cursor.advance();
            Ok(Token::Percent)
        }
        '.' => {
            cursor.advance();
            Ok(Token::Dot)
        }
        '"' => scan_string(cursor),
        '$' => scan_variable(cursor),
        '0'..='9' => scan_integer(cursor),
        'a'..='z' | 'A'..='Z' | '_' => scan_keyword(cursor),
        _ => Err(CompileError::new(
            cursor.span(),
            &format!("Unexpected character: '{}'", ch),
        )),
    }
}

fn scan_string(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let span = cursor.span();
    cursor.advance(); // opening "

    let mut value = String::new();

    loop {
        match cursor.advance() {
            Some('"') => return Ok(Token::StringLiteral(value)),
            Some('\\') => match cursor.advance() {
                Some('n') => value.push('\n'),
                Some('t') => value.push('\t'),
                Some('\\') => value.push('\\'),
                Some('"') => value.push('"'),
                Some(c) => {
                    value.push('\\');
                    value.push(c);
                }
                None => {
                    return Err(CompileError::new(span, "Unterminated string literal"))
                }
            },
            Some(c) => value.push(c),
            None => return Err(CompileError::new(span, "Unterminated string literal")),
        }
    }
}

fn scan_variable(cursor: &mut Cursor) -> Result<Token, CompileError> {
    cursor.advance(); // $
    let mut name = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    if name.is_empty() {
        return Err(CompileError::new(
            cursor.span(),
            "Expected variable name after '$'",
        ));
    }

    Ok(Token::Variable(name))
}

fn scan_integer(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut num_str = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    let value: i64 = num_str.parse().map_err(|_| {
        CompileError::new(cursor.span(), "Invalid integer literal")
    })?;

    Ok(Token::IntLiteral(value))
}

fn scan_keyword(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut word = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    match word.as_str() {
        "echo" => Ok(Token::Echo),
        "if" => Ok(Token::If),
        "else" => Ok(Token::Else),
        "elseif" => Ok(Token::ElseIf),
        "while" => Ok(Token::While),
        "for" => Ok(Token::For),
        "break" => Ok(Token::Break),
        "continue" => Ok(Token::Continue),
        _ => Err(CompileError::new(
            cursor.span(),
            &format!("Unknown keyword: '{}'", word),
        )),
    }
}
