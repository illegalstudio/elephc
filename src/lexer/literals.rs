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
                    Some('0') => current.push('\0'),
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

    if name == "this" {
        return Ok(Token::This);
    }

    Ok(Token::Variable(name))
}

/// Collect digits according to `is_digit`, allowing a single `_` between digits
/// (PHP 7.4+ numeric separator). The helper never consumes a leading or trailing
/// `_` — those remain on the cursor so [`validate_no_trailing_alnum`] can flag
/// them. Returns the digit string with separators stripped.
fn scan_radix_digits<F: Fn(char) -> bool>(cursor: &mut Cursor, is_digit: F) -> String {
    let mut s = String::new();
    while let Some(ch) = cursor.peek() {
        if is_digit(ch) {
            s.push(ch);
            cursor.advance();
        } else if ch == '_' && !s.is_empty() {
            let remaining = cursor.remaining();
            let next_is_digit =
                remaining.len() > 1 && is_digit(remaining.as_bytes()[1] as char);
            if next_is_digit {
                cursor.advance();
            } else {
                break;
            }
        } else {
            break;
        }
    }
    s
}

/// After scanning a numeric literal, ensure no alphanumeric character or `_`
/// follows. Catches malformed forms like `0o78`, `078`, `0xfg`, `0b12`, `1_`,
/// and `1__0`, which PHP rejects at parse time but the lexer would otherwise
/// silently split into two adjacent tokens.
fn validate_no_trailing_alnum(cursor: &Cursor, base_label: &str) -> Result<(), CompileError> {
    if let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            return Err(CompileError::new(
                cursor.span(),
                &format!("Unexpected character '{ch}' after {base_label} literal"),
            ));
        }
    }
    Ok(())
}

pub fn scan_number(cursor: &mut Cursor) -> Result<Token, CompileError> {
    // Prefixed integer literals: 0x / 0o / 0b (and uppercase variants)
    if cursor.peek() == Some('0') {
        let remaining = cursor.remaining();
        if remaining.len() > 1 {
            let prefix = remaining.as_bytes()[1];

            // Hexadecimal (0x or 0X)
            if prefix == b'x' || prefix == b'X' {
                cursor.advance(); // consume '0'
                cursor.advance(); // consume 'x' or 'X'
                let hex_str = scan_radix_digits(cursor, |c| c.is_ascii_hexdigit());
                if hex_str.is_empty() {
                    return Err(CompileError::new(
                        cursor.span(),
                        "Expected hex digits after '0x'",
                    ));
                }
                validate_no_trailing_alnum(cursor, "hex")?;
                let value = i64::from_str_radix(&hex_str, 16)
                    .map_err(|_| CompileError::new(cursor.span(), "Invalid hex literal"))?;
                return Ok(Token::IntLiteral(value));
            }

            // Explicit octal (0o or 0O) — PHP 8.1+
            if prefix == b'o' || prefix == b'O' {
                cursor.advance();
                cursor.advance();
                let octal_str = scan_radix_digits(cursor, |c| c.is_ascii_digit() && c < '8');
                if octal_str.is_empty() {
                    return Err(CompileError::new(
                        cursor.span(),
                        "Expected octal digits after '0o'",
                    ));
                }
                validate_no_trailing_alnum(cursor, "octal")?;
                let value = i64::from_str_radix(&octal_str, 8)
                    .map_err(|_| CompileError::new(cursor.span(), "Invalid octal literal"))?;
                return Ok(Token::IntLiteral(value));
            }

            // Binary (0b or 0B) — PHP 5.4+
            if prefix == b'b' || prefix == b'B' {
                cursor.advance();
                cursor.advance();
                let bin_str = scan_radix_digits(cursor, |c| c == '0' || c == '1');
                if bin_str.is_empty() {
                    return Err(CompileError::new(
                        cursor.span(),
                        "Expected binary digits after '0b'",
                    ));
                }
                validate_no_trailing_alnum(cursor, "binary")?;
                let value = i64::from_str_radix(&bin_str, 2)
                    .map_err(|_| CompileError::new(cursor.span(), "Invalid binary literal"))?;
                return Ok(Token::IntLiteral(value));
            }
        }
    }

    let mut num_str = scan_radix_digits(cursor, |c| c.is_ascii_digit());

    // Check for decimal point followed by digit (float literal)
    let is_float = if cursor.peek() == Some('.') {
        let remaining = cursor.remaining();
        remaining.len() > 1 && (remaining.as_bytes()[1] as char).is_ascii_digit()
    } else {
        false
    };

    // Check for scientific notation without decimal point (e.g. 1e5)
    let is_sci = matches!(cursor.peek(), Some('e') | Some('E'));

    if is_float || is_sci {
        if is_float {
            num_str.push('.');
            cursor.advance(); // consume '.'
            num_str.push_str(&scan_radix_digits(cursor, |c| c.is_ascii_digit()));
        }
        // Scientific notation
        if matches!(cursor.peek(), Some('e') | Some('E')) {
            num_str.push('e');
            cursor.advance();
            if let Some(sign @ ('+' | '-')) = cursor.peek() {
                num_str.push(sign);
                cursor.advance();
            }
            num_str.push_str(&scan_radix_digits(cursor, |c| c.is_ascii_digit()));
        }
        validate_no_trailing_alnum(cursor, "float")?;
        let value: f64 = num_str
            .parse()
            .map_err(|_| CompileError::new(cursor.span(), "Invalid float literal"))?;
        return Ok(Token::FloatLiteral(value));
    }

    let is_legacy_octal = num_str.len() > 1 && num_str.starts_with('0');
    validate_no_trailing_alnum(
        cursor,
        if is_legacy_octal { "octal" } else { "decimal" },
    )?;
    if is_legacy_octal {
        let value = i64::from_str_radix(&num_str, 8)
            .map_err(|_| CompileError::new(cursor.span(), "Invalid octal literal"))?;
        return Ok(Token::IntLiteral(value));
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

    num_str.push_str(&scan_radix_digits(cursor, |c| c.is_ascii_digit()));

    // Scientific notation
    if matches!(cursor.peek(), Some('e') | Some('E')) {
        num_str.push('e');
        cursor.advance();
        if let Some(sign @ ('+' | '-')) = cursor.peek() {
            num_str.push(sign);
            cursor.advance();
        }
        num_str.push_str(&scan_radix_digits(cursor, |c| c.is_ascii_digit()));
    }

    validate_no_trailing_alnum(cursor, "float")?;

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

    if word.eq_ignore_ascii_case("__DIR__") {
        return Ok(Token::DunderDir);
    }
    if word.eq_ignore_ascii_case("__FILE__") {
        return Ok(Token::DunderFile);
    }
    if word.eq_ignore_ascii_case("__LINE__") {
        return Ok(Token::DunderLine);
    }
    if word.eq_ignore_ascii_case("__FUNCTION__") {
        return Ok(Token::DunderFunction);
    }
    if word.eq_ignore_ascii_case("__CLASS__") {
        return Ok(Token::DunderClass);
    }
    if word.eq_ignore_ascii_case("__METHOD__") {
        return Ok(Token::DunderMethod);
    }
    if word.eq_ignore_ascii_case("__NAMESPACE__") {
        return Ok(Token::DunderNamespace);
    }
    if word.eq_ignore_ascii_case("__TRAIT__") {
        return Ok(Token::DunderTrait);
    }

    match word.as_str() {
        "INF" => return Ok(Token::Inf),
        "NAN" => return Ok(Token::Nan),
        "PHP_INT_MAX" => return Ok(Token::PhpIntMax),
        "PHP_INT_MIN" => return Ok(Token::PhpIntMin),
        "PHP_FLOAT_MAX" => return Ok(Token::PhpFloatMax),
        "M_PI" => return Ok(Token::MPi),
        "M_E" => return Ok(Token::ME),
        "M_SQRT2" => return Ok(Token::MSqrt2),
        "M_PI_2" => return Ok(Token::MPi2),
        "M_PI_4" => return Ok(Token::MPi4),
        "M_LOG2E" => return Ok(Token::MLog2e),
        "M_LOG10E" => return Ok(Token::MLog10e),
        "PHP_FLOAT_MIN" => return Ok(Token::PhpFloatMin),
        "PHP_FLOAT_EPSILON" => return Ok(Token::PhpFloatEpsilon),
        "STDIN" => return Ok(Token::Stdin),
        "STDOUT" => return Ok(Token::Stdout),
        "STDERR" => return Ok(Token::Stderr),
        "PHP_EOL" => return Ok(Token::PhpEol),
        "PHP_OS" => return Ok(Token::PhpOs),
        "DIRECTORY_SEPARATOR" => return Ok(Token::DirectorySeparator),
        _ => {}
    }

    match word.to_ascii_lowercase().as_str() {
        "and" => Ok(Token::And),
        "or" => Ok(Token::Or),
        "xor" => Ok(Token::Xor),
        "instanceof" => Ok(Token::InstanceOf),
        "echo" => Ok(Token::Echo),
        "if" => Ok(Token::If),
        "ifdef" => Ok(Token::IfDef),
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
        "try" => Ok(Token::Try),
        "catch" => Ok(Token::Catch),
        "finally" => Ok(Token::Finally),
        "throw" => Ok(Token::Throw),
        "extends" => Ok(Token::Extends),
        "implements" => Ok(Token::Implements),
        "interface" => Ok(Token::Interface),
        "abstract" => Ok(Token::Abstract),
        "final" => Ok(Token::Final),
        "print" => Ok(Token::Print),
        "switch" => Ok(Token::Switch),
        "case" => Ok(Token::Case),
        "default" => Ok(Token::Default),
        "match" => Ok(Token::Match),
        "include" => Ok(Token::Include),
        "include_once" => Ok(Token::IncludeOnce),
        "require" => Ok(Token::Require),
        "require_once" => Ok(Token::RequireOnce),
        "fn" => Ok(Token::Fn),
        "use" => Ok(Token::Use),
        "namespace" => Ok(Token::Namespace),
        "const" => Ok(Token::Const),
        "global" => Ok(Token::Global),
        "static" => Ok(Token::Static),
        "self" => Ok(Token::Self_),
        "trait" => Ok(Token::Trait),
        "parent" => Ok(Token::Parent),
        "insteadof" => Ok(Token::InsteadOf),
        "class" => Ok(Token::Class),
        "enum" => Ok(Token::Enum),
        "new" => Ok(Token::New),
        "public" => Ok(Token::Public),
        "protected" => Ok(Token::Protected),
        "private" => Ok(Token::Private),
        "readonly" => Ok(Token::ReadOnly),
        "extern" => Ok(Token::Extern),
        "packed" => Ok(Token::Packed),
        _ => Ok(Token::Identifier(word)),
    }
}

/// Scan a heredoc or nowdoc string.
/// At this point, `<<<` has already been consumed.
/// Heredoc: `<<<LABEL` or `<<<\"LABEL\"` — supports variable interpolation like double-quoted strings
/// Nowdoc: `<<<'LABEL'` — no interpolation (like single-quoted strings)
pub fn scan_heredoc(
    cursor: &mut Cursor,
) -> Result<Vec<(Token, crate::span::Span)>, CompileError> {
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

                // For heredoc: process escape sequences and variable interpolation
                // For nowdoc: return raw content (no processing)
                if is_nowdoc {
                    return Ok(vec![(Token::StringLiteral(content), span)]);
                }

                // Heredoc: process escape sequences and variable interpolation together
                // (must be done in one pass so \$ is treated as literal $, not interpolation)
                return Ok(interpolate_heredoc_content(&content, span));
            }
        }

        // Read one character of content
        match cursor.advance() {
            Some(ch) => content.push(ch),
            None => return Err(CompileError::new(span, "Unterminated heredoc/nowdoc")),
        }
    }
}

/// Interpolate variables and process escape sequences in heredoc content.
/// Handles both in a single pass so that `\$` produces a literal `$` without triggering
/// variable interpolation. Scans for `$identifier` patterns and expands them into
/// concatenation tokens: `Hello $name!` -> `("Hello " . $name . "!")`
fn interpolate_heredoc_content(
    content: &str,
    span: crate::span::Span,
) -> Vec<(Token, crate::span::Span)> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_interpolation = false;
    let mut chars = content.chars().peekable();

    loop {
        match chars.peek() {
            None => break,
            Some(&'\\') => {
                chars.next(); // consume backslash
                match chars.peek() {
                    Some(&'n') => { chars.next(); current.push('\n'); }
                    Some(&'t') => { chars.next(); current.push('\t'); }
                    Some(&'\\') => { chars.next(); current.push('\\'); }
                    Some(&'"') => { chars.next(); current.push('"'); }
                    Some(&'$') => { chars.next(); current.push('$'); }
                    Some(&'0') => { chars.next(); current.push('\0'); }
                    Some(&c) => { chars.next(); current.push('\\'); current.push(c); }
                    None => current.push('\\'),
                }
            }
            Some(&'$') => {
                chars.next(); // consume '$'
                let mut name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        name.push(ch);
                        chars.next();
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
                        tokens.push((
                            Token::StringLiteral(std::mem::take(&mut current)),
                            span,
                        ));
                    }
                    // Add dot + variable
                    if !tokens.is_empty() && !matches!(tokens.last(), Some((Token::Dot, _))) {
                        tokens.push((Token::Dot, span));
                    }
                    tokens.push((Token::Variable(name), span));
                }
            }
            Some(&ch) => {
                current.push(ch);
                chars.next();
            }
        }
    }

    if !has_interpolation {
        // No interpolation — return single StringLiteral
        return vec![(Token::StringLiteral(current), span)];
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
    result
}
