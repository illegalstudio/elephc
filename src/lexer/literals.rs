use super::cursor::Cursor;
use super::token::Token;
use crate::errors::CompileError;

/// Scan a double-quoted string with interpolation support.
/// Returns one or more tokens: for `"Hello $name!"` it returns
/// `StringLiteral("Hello ") . Variable("name") . StringLiteral("!")`
/// (with Dot tokens for concatenation).
pub fn scan_double_string_interpolated(
    cursor: &mut Cursor,
) -> Result<Vec<(Token, crate::span::Span)>, CompileError> {
    let span = cursor.span();
    cursor.advance(); // opening "

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_interpolation = false;

    loop {
        match cursor.peek() {
            Some('"') => {
                cursor.advance();
                break;
            }
            Some('\\') => {
                cursor.advance();
                match cursor.advance() {
                    Some('n') => current.push('\n'),
                    Some('t') => current.push('\t'),
                    Some('\\') => current.push('\\'),
                    Some('"') => current.push('"'),
                    Some('$') => current.push('$'),
                    Some(c) => {
                        current.push('\\');
                        current.push(c);
                    }
                    None => return Err(CompileError::new(span, "Unterminated string literal")),
                }
            }
            Some('$') => {
                // Variable interpolation
                cursor.advance(); // consume '$'
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
                    // Just a literal '$' (no valid variable name follows)
                    current.push('$');
                } else {
                    has_interpolation = true;
                    // Flush accumulated string
                    if !current.is_empty() || tokens.is_empty() {
                        if !tokens.is_empty() {
                            tokens.push((Token::Dot, span));
                        }
                        tokens.push((Token::StringLiteral(std::mem::take(&mut current)), span));
                    }
                    // Add dot + variable
                    if !tokens.is_empty() && !matches!(tokens.last(), Some((Token::Dot, _))) {
                        tokens.push((Token::Dot, span));
                    }
                    tokens.push((Token::Variable(name), span));
                }
            }
            Some(c) => {
                current.push(c);
                cursor.advance();
            }
            None => return Err(CompileError::new(span, "Unterminated string literal")),
        }
    }

    if !has_interpolation {
        // No interpolation — return single StringLiteral
        return Ok(vec![(Token::StringLiteral(current), span)]);
    }

    // Flush remaining string
    if !current.is_empty() {
        tokens.push((Token::Dot, span));
        tokens.push((Token::StringLiteral(current), span));
    }

    // Wrap in parens so precedence is correct: ("..." . $var . "...")
    let mut result = vec![(Token::LParen, span)];
    result.extend(tokens);
    result.push((Token::RParen, span));
    Ok(result)
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

pub fn scan_number(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut num_str = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    // Check for decimal point followed by digit (float literal)
    let is_float = if cursor.peek() == Some('.') {
        // Look ahead: need a digit after the dot to be a float
        let remaining = cursor.remaining();
        remaining.len() > 1 && remaining.as_bytes()[1].is_ascii_digit()
    } else {
        false
    };

    // Check for scientific notation without decimal point (e.g. 1e5)
    let is_sci = matches!(cursor.peek(), Some('e') | Some('E'));

    if is_float || is_sci {
        if is_float {
            num_str.push('.');
            cursor.advance(); // consume '.'
            while let Some(ch) = cursor.peek() {
                if ch.is_ascii_digit() {
                    num_str.push(ch);
                    cursor.advance();
                } else {
                    break;
                }
            }
        }
        // Scientific notation
        if matches!(cursor.peek(), Some('e') | Some('E')) {
            num_str.push('e');
            cursor.advance();
            if matches!(cursor.peek(), Some('+') | Some('-')) {
                num_str.push(cursor.peek().unwrap());
                cursor.advance();
            }
            while let Some(ch) = cursor.peek() {
                if ch.is_ascii_digit() {
                    num_str.push(ch);
                    cursor.advance();
                } else {
                    break;
                }
            }
        }
        let value: f64 = num_str
            .parse()
            .map_err(|_| CompileError::new(cursor.span(), "Invalid float literal"))?;
        return Ok(Token::FloatLiteral(value));
    }

    let value: i64 = num_str
        .parse()
        .map_err(|_| CompileError::new(cursor.span(), "Invalid integer literal"))?;

    Ok(Token::IntLiteral(value))
}

/// Scan a float literal starting with `.` (e.g., `.5`, `.123`)
pub fn scan_dot_float(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut num_str = String::from("0.");
    cursor.advance(); // consume '.'

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    // Scientific notation
    if matches!(cursor.peek(), Some('e') | Some('E')) {
        num_str.push('e');
        cursor.advance();
        if matches!(cursor.peek(), Some('+') | Some('-')) {
            num_str.push(cursor.peek().unwrap());
            cursor.advance();
        }
        while let Some(ch) = cursor.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                cursor.advance();
            } else {
                break;
            }
        }
    }

    let value: f64 = num_str
        .parse()
        .map_err(|_| CompileError::new(cursor.span(), "Invalid float literal"))?;

    Ok(Token::FloatLiteral(value))
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
        "INF" => Ok(Token::Inf),
        "NAN" => Ok(Token::Nan),
        "PHP_INT_MAX" => Ok(Token::PhpIntMax),
        "PHP_INT_MIN" => Ok(Token::PhpIntMin),
        "PHP_FLOAT_MAX" => Ok(Token::PhpFloatMax),
        "M_PI" => Ok(Token::MPi),
        "print" => Ok(Token::Print),
        "switch" => Ok(Token::Switch),
        "case" => Ok(Token::Case),
        "default" => Ok(Token::Default),
        "match" => Ok(Token::Match),
        "include" => Ok(Token::Include),
        "include_once" => Ok(Token::IncludeOnce),
        "require" => Ok(Token::Require),
        "require_once" => Ok(Token::RequireOnce),
        "STDIN" => Ok(Token::Stdin),
        "STDOUT" => Ok(Token::Stdout),
        "STDERR" => Ok(Token::Stderr),
        "fn" => Ok(Token::Fn),
        "use" => Ok(Token::Use),
        "const" => Ok(Token::Const),
        _ => Ok(Token::Identifier(word)),
    }
}

/// Scan a heredoc or nowdoc string.
/// At this point, `<<<` has already been consumed.
/// Heredoc: `<<<LABEL` or `<<<\"LABEL\"`
/// Nowdoc: `<<<'LABEL'`
pub fn scan_heredoc(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let span = cursor.span();

    // Skip optional whitespace between <<< and identifier
    while cursor.peek() == Some(' ') || cursor.peek() == Some('\t') {
        cursor.advance();
    }

    // Check for nowdoc (single-quoted label)
    let is_nowdoc = cursor.peek() == Some('\'');
    let is_quoted_heredoc = cursor.peek() == Some('"');

    if is_nowdoc || is_quoted_heredoc {
        cursor.advance(); // consume opening quote
    }

    // Read the label
    let mut label = String::new();
    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            label.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    if label.is_empty() {
        return Err(CompileError::new(span, "Expected heredoc/nowdoc label after '<<<'"));
    }

    // Consume closing quote if present
    if is_nowdoc {
        if cursor.peek() != Some('\'') {
            return Err(CompileError::new(span, "Expected closing ' for nowdoc label"));
        }
        cursor.advance();
    } else if is_quoted_heredoc {
        if cursor.peek() != Some('"') {
            return Err(CompileError::new(span, "Expected closing \" for heredoc label"));
        }
        cursor.advance();
    }

    // Consume newline after label
    if cursor.peek() == Some('\r') {
        cursor.advance();
    }
    if cursor.peek() == Some('\n') {
        cursor.advance();
    } else {
        return Err(CompileError::new(span, "Expected newline after heredoc/nowdoc label"));
    }

    // Read content until we find the closing label on its own line
    let mut content = String::new();
    loop {
        if cursor.is_eof() {
            return Err(CompileError::new(span, "Unterminated heredoc/nowdoc"));
        }

        // Check if current line starts with optional whitespace then the closing label
        let remaining = cursor.remaining();

        // Count leading whitespace
        let mut ws_count = 0;
        for b in remaining.bytes() {
            if b == b' ' || b == b'\t' {
                ws_count += 1;
            } else {
                break;
            }
        }

        let after_ws = &remaining[ws_count..];
        if after_ws.starts_with(&label) {
            let after_label = &after_ws[label.len()..];
            // Label must be followed by ; or newline or EOF
            if after_label.is_empty()
                || after_label.starts_with(';')
                || after_label.starts_with('\n')
                || after_label.starts_with('\r')
            {
                // Found closing label — consume whitespace + label
                for _ in 0..ws_count {
                    cursor.advance();
                }
                for _ in 0..label.len() {
                    cursor.advance();
                }
                // Remove trailing newline from content
                if content.ends_with('\n') {
                    content.pop();
                    if content.ends_with('\r') {
                        content.pop();
                    }
                }

                // For heredoc: process escape sequences
                if !is_nowdoc {
                    content = process_heredoc_escapes(&content);
                }

                return Ok(Token::StringLiteral(content));
            }
        }

        // Read one character of content
        match cursor.advance() {
            Some(ch) => content.push(ch),
            None => return Err(CompileError::new(span, "Unterminated heredoc/nowdoc")),
        }
    }
}

/// Process escape sequences in heredoc content (same as double-quoted strings).
fn process_heredoc_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('$') => result.push('$'),
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}
