use super::cursor::Cursor;
use super::token::Token;
use crate::errors::CompileError;

pub fn scan_double_string(cursor: &mut Cursor) -> Result<Token, CompileError> {
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
                None => return Err(CompileError::new(span, "Unterminated string literal")),
            },
            Some(c) => value.push(c),
            None => return Err(CompileError::new(span, "Unterminated string literal")),
        }
    }
}

pub fn scan_single_string(cursor: &mut Cursor) -> Result<Token, CompileError> {
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

pub fn scan_variable(cursor: &mut Cursor) -> Result<Token, CompileError> {
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
        return Err(CompileError::new(cursor.span(), "Expected variable name after '$'"));
    }

    Ok(Token::Variable(name))
}

pub fn scan_integer(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut num_str = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    let value: i64 = num_str
        .parse()
        .map_err(|_| CompileError::new(cursor.span(), "Invalid integer literal"))?;

    Ok(Token::IntLiteral(value))
}

pub fn scan_keyword(cursor: &mut Cursor) -> Result<Token, CompileError> {
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
        "foreach" => Ok(Token::Foreach),
        "as" => Ok(Token::As),
        _ => Ok(Token::Identifier(word)),
    }
}
