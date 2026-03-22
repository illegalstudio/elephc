use super::cursor::Cursor;
use super::token::Token;
use crate::errors::CompileError;
use crate::span::Span;

pub fn scan_tokens(source: &str) -> Result<Vec<(Token, Span)>, CompileError> {
    let mut cursor = Cursor::new(source);
    let mut tokens = Vec::new();

    skip_whitespace_and_comments(&mut cursor);

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
        skip_whitespace_and_comments(&mut cursor);

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

fn skip_whitespace_and_comments(cursor: &mut Cursor) {
    loop {
        // Skip whitespace
        while let Some(ch) = cursor.peek() {
            if ch.is_ascii_whitespace() {
                cursor.advance();
            } else {
                break;
            }
        }

        // Skip line comment
        if cursor.remaining().starts_with("//") {
            while let Some(ch) = cursor.advance() {
                if ch == '\n' {
                    break;
                }
            }
            continue;
        }

        // Skip block comment
        if cursor.remaining().starts_with("/*") {
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
            continue;
        }

        break;
    }
}

fn scan_token(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let ch = cursor.peek().unwrap();

    match ch {
        ';' => {
            cursor.advance();
            Ok(Token::Semicolon)
        }
        ',' => {
            cursor.advance();
            Ok(Token::Comma)
        }
        '?' => {
            cursor.advance();
            Ok(Token::Question)
        }
        ':' => {
            cursor.advance();
            Ok(Token::Colon)
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
                Ok(Token::Bang)
            }
        }
        '&' => {
            cursor.advance();
            if cursor.peek() == Some('&') {
                cursor.advance();
                Ok(Token::AndAnd)
            } else {
                Err(CompileError::new(cursor.span(), "Expected '&' after '&'"))
            }
        }
        '|' => {
            cursor.advance();
            if cursor.peek() == Some('|') {
                cursor.advance();
                Ok(Token::OrOr)
            } else {
                Err(CompileError::new(cursor.span(), "Expected '|' after '|'"))
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
            match cursor.peek() {
                Some('+') => { cursor.advance(); Ok(Token::PlusPlus) }
                Some('=') => { cursor.advance(); Ok(Token::PlusAssign) }
                _ => Ok(Token::Plus),
            }
        }
        '-' => {
            cursor.advance();
            match cursor.peek() {
                Some('-') => { cursor.advance(); Ok(Token::MinusMinus) }
                Some('=') => { cursor.advance(); Ok(Token::MinusAssign) }
                _ => Ok(Token::Minus),
            }
        }
        '*' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::StarAssign)
            } else {
                Ok(Token::Star)
            }
        }
        '/' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::SlashAssign)
            } else {
                Ok(Token::Slash)
            }
        }
        '%' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::PercentAssign)
            } else {
                Ok(Token::Percent)
            }
        }
        '.' => {
            cursor.advance();
            if cursor.peek() == Some('=') {
                cursor.advance();
                Ok(Token::DotAssign)
            } else {
                Ok(Token::Dot)
            }
        }
        '"' => scan_double_string(cursor),
        '\'' => scan_single_string(cursor),
        '$' => scan_variable(cursor),
        '0'..='9' => scan_integer(cursor),
        'a'..='z' | 'A'..='Z' | '_' => scan_keyword(cursor),
        _ => Err(CompileError::new(
            cursor.span(),
            &format!("Unexpected character: '{}'", ch),
        )),
    }
}

fn scan_double_string(cursor: &mut Cursor) -> Result<Token, CompileError> {
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

fn scan_single_string(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let span = cursor.span();
    cursor.advance(); // opening '

    let mut value = String::new();

    loop {
        match cursor.advance() {
            Some('\'') => return Ok(Token::StringLiteral(value)),
            Some('\\') => match cursor.peek() {
                Some('\'') => {
                    cursor.advance();
                    value.push('\'');
                }
                Some('\\') => {
                    cursor.advance();
                    value.push('\\');
                }
                _ => value.push('\\'),
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
        "function" => Ok(Token::Function),
        "return" => Ok(Token::Return),
        "true" => Ok(Token::True),
        "false" => Ok(Token::False),
        "null" => Ok(Token::Null),
        "do" => Ok(Token::Do),
        _ => Ok(Token::Identifier(word)),
    }
}
