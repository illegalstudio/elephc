//! Purpose:
//! Tiny side-effect-free expression evaluator used by DAP conditional
//! breakpoints. It reads locals from the stopped activation and supports
//! literals, comparisons, boolean operators and the four basic arithmetic
//! operators; it never calls PHP code or mutates the debuggee.
//!
//! Called from:
//! - `crate::dap::Debugger::dap_set_breakpoints` for syntax validation;
//! - `crate::elephc_debug_site_inner` through `Debugger::evaluate_condition`
//!   before a conditional breakpoint stop.
//!
//! Key details:
//! - Bare identifiers and `$name` both resolve to frame locals.
//! - `===`/`!==` compare types and values; `==`/`!=` follow the reduced PHP
//!   scalar rules implemented below. Arrays/objects are `Other` and only
//!   participate in truthiness.
//! - A malformed expression or an unknown local is an error; the runtime skips
//!   that breakpoint and reports the condition failure.

use super::{value, Debugger};

/// One token in a condition expression.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Int(i64),
    Float(f64),
    String(String),
    Ident(String),
    Var(String),
    True,
    False,
    Null,
    LParen,
    RParen,
    Not,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    End,
}

/// Tokenizes one condition expression.
fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'$' {
            index += 1;
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            if start == index {
                return Err("expected a variable name after '$'".to_string());
            }
            let name = String::from_utf8_lossy(&bytes[start..index]).into_owned();
            tokens.push(Token::Var(name));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let word = String::from_utf8_lossy(&bytes[start..index]).into_owned();
            tokens.push(match word.as_str() {
                "true" => Token::True,
                "false" => Token::False,
                "null" => Token::Null,
                _ => Token::Ident(word),
            });
            continue;
        }
        if byte.is_ascii_digit() || (byte == b'.' && index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit()) {
            let start = index;
            let mut float = false;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'.' {
                float = true;
                index += 1;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
            }
            if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
                float = true;
                index += 1;
                if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
                    index += 1;
                }
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
            }
            let text = String::from_utf8_lossy(&bytes[start..index]);
            if float {
                let value = text
                    .parse::<f64>()
                    .map_err(|_| format!("invalid float '{text}'"))?;
                tokens.push(Token::Float(value));
            } else {
                let value = text
                    .parse::<i64>()
                    .map_err(|_| format!("invalid integer '{text}'"))?;
                tokens.push(Token::Int(value));
            }
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let quote = byte;
            index += 1;
            let mut text = String::new();
            let mut closed = false;
            while index < bytes.len() {
                let current = bytes[index];
                index += 1;
                if current == quote {
                    closed = true;
                    break;
                }
                if current == b'\\' && index < bytes.len() {
                    let escaped = bytes[index];
                    index += 1;
                    match escaped {
                        b'n' => text.push('\n'),
                        b'r' => text.push('\r'),
                        b't' => text.push('\t'),
                        b'0' => text.push('\0'),
                        b'\\' => text.push('\\'),
                        b'"' => text.push('"'),
                        b'\'' => text.push('\''),
                        other => text.push(other as char),
                    }
                } else {
                    text.push(current as char);
                }
            }
            if !closed {
                return Err("unterminated string literal".to_string());
            }
            tokens.push(Token::String(text));
            continue;
        }
        let (token, length) = match byte {
            b'(' => (Token::LParen, 1),
            b')' => (Token::RParen, 1),
            b'!' if bytes.get(index + 1) == Some(&b'=') && bytes.get(index + 2) == Some(&b'=') => {
                (Token::NotEqEq, 3)
            }
            b'!' if bytes.get(index + 1) == Some(&b'=') => (Token::NotEq, 2),
            b'!' => (Token::Not, 1),
            b'=' if bytes.get(index + 1) == Some(&b'=') && bytes.get(index + 2) == Some(&b'=') => {
                (Token::EqEqEq, 3)
            }
            b'=' if bytes.get(index + 1) == Some(&b'=') => (Token::EqEq, 2),
            b'=' => return Err("assignment is not allowed in a condition".to_string()),
            b'<' if bytes.get(index + 1) == Some(&b'=') => (Token::Le, 2),
            b'<' => (Token::Lt, 1),
            b'>' if bytes.get(index + 1) == Some(&b'=') => (Token::Ge, 2),
            b'>' => (Token::Gt, 1),
            b'+' => (Token::Plus, 1),
            b'-' => (Token::Minus, 1),
            b'*' => (Token::Star, 1),
            b'/' => (Token::Slash, 1),
            b'%' => (Token::Percent, 1),
            b'&' if bytes.get(index + 1) == Some(&b'&') => (Token::AndAnd, 2),
            b'|' if bytes.get(index + 1) == Some(&b'|') => (Token::OrOr, 2),
            other => {
                return Err(format!(
                    "unsupported character '{}' in condition",
                    other as char
                ))
            }
        };
        tokens.push(token);
        index += length;
    }
    tokens.push(Token::End);
    Ok(tokens)
}

/// Recursive-descent parser/evaluator over one token stream.
struct Parser<F>
where
    F: Fn(&str) -> Result<value::Scalar, String>,
{
    tokens: Vec<Token>,
    index: usize,
    lookup: F,
}

impl<F> Parser<F>
where
    F: Fn(&str) -> Result<value::Scalar, String>,
{
    /// Tokenizes `condition` and prepares the evaluator.
    fn new(condition: &str, lookup: F) -> Result<Self, String> {
        Ok(Self {
            tokens: tokenize(condition)?,
            index: 0,
            lookup,
        })
    }

    /// Returns the current token without consuming it.
    fn peek(&self) -> &Token {
        self.tokens.get(self.index).unwrap_or(&Token::End)
    }

    /// Consumes and returns the current token.
    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        self.index += 1;
        token
    }

    /// Parses one complete expression and requires the stream to be exhausted.
    fn parse(&mut self) -> Result<value::Scalar, String> {
        let value = self.parse_or()?;
        if self.peek() != &Token::End {
            return Err("unexpected token after condition".to_string());
        }
        Ok(value)
    }

    /// `||` has the lowest precedence.
    fn parse_or(&mut self) -> Result<value::Scalar, String> {
        let mut left = self.parse_and()?;
        while self.peek() == &Token::OrOr {
            self.next();
            let right = self.parse_and()?;
            left = value::Scalar::Bool(truthy(&left) || truthy(&right));
        }
        Ok(left)
    }

    /// `&&` binds tighter than `||`.
    fn parse_and(&mut self) -> Result<value::Scalar, String> {
        let mut left = self.parse_comparison()?;
        while self.peek() == &Token::AndAnd {
            self.next();
            let right = self.parse_comparison()?;
            left = value::Scalar::Bool(truthy(&left) && truthy(&right));
        }
        Ok(left)
    }

    /// Comparison operators.
    fn parse_comparison(&mut self) -> Result<value::Scalar, String> {
        let mut left = self.parse_additive()?;
        loop {
            let operator = self.peek().clone();
            if !matches!(
                operator,
                Token::EqEq
                    | Token::EqEqEq
                    | Token::NotEq
                    | Token::NotEqEq
                    | Token::Lt
                    | Token::Le
                    | Token::Gt
                    | Token::Ge
            ) {
                break;
            }
            self.next();
            let right = self.parse_additive()?;
            left = value::Scalar::Bool(match operator {
                Token::EqEq => loose_eq(&left, &right),
                Token::EqEqEq => strict_eq(&left, &right),
                Token::NotEq => !loose_eq(&left, &right),
                Token::NotEqEq => !strict_eq(&left, &right),
                Token::Lt => compare(&left, &right)? < 0.0,
                Token::Le => compare(&left, &right)? <= 0.0,
                Token::Gt => compare(&left, &right)? > 0.0,
                Token::Ge => compare(&left, &right)? >= 0.0,
                _ => unreachable!(),
            });
        }
        Ok(left)
    }

    /// Addition and subtraction.
    fn parse_additive(&mut self) -> Result<value::Scalar, String> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let operator = self.peek().clone();
            if !matches!(operator, Token::Plus | Token::Minus) {
                break;
            }
            self.next();
            let right = self.parse_multiplicative()?;
            left = if operator == Token::Plus {
                add(&left, &right)?
            } else {
                subtract(&left, &right)?
            };
        }
        Ok(left)
    }

    /// Multiplication, division and modulo.
    fn parse_multiplicative(&mut self) -> Result<value::Scalar, String> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = self.peek().clone();
            if !matches!(operator, Token::Star | Token::Slash | Token::Percent) {
                break;
            }
            self.next();
            let right = self.parse_unary()?;
            left = match operator {
                Token::Star => multiply(&left, &right)?,
                Token::Slash => divide(&left, &right)?,
                Token::Percent => modulo(&left, &right)?,
                _ => unreachable!(),
            };
        }
        Ok(left)
    }

    /// Unary `!`, `+` and `-`.
    fn parse_unary(&mut self) -> Result<value::Scalar, String> {
        match self.peek() {
            Token::Not => {
                self.next();
                Ok(value::Scalar::Bool(!truthy(&self.parse_unary()?)))
            }
            Token::Plus => {
                self.next();
                self.parse_unary()
            }
            Token::Minus => {
                self.next();
                negate(&self.parse_unary()?)
            }
            _ => self.parse_primary(),
        }
    }

    /// Literal, local or parenthesized expression.
    fn parse_primary(&mut self) -> Result<value::Scalar, String> {
        match self.next() {
            Token::Int(value) => Ok(value::Scalar::Int(value)),
            Token::Float(value) => Ok(value::Scalar::Float(value)),
            Token::String(value) => Ok(value::Scalar::String(value)),
            Token::True => Ok(value::Scalar::Bool(true)),
            Token::False => Ok(value::Scalar::Bool(false)),
            Token::Null => Ok(value::Scalar::Null),
            Token::Var(name) | Token::Ident(name) => (self.lookup)(&name),
            Token::LParen => {
                let value = self.parse_or()?;
                if self.next() != Token::RParen {
                    return Err("expected ')' in condition".to_string());
                }
                Ok(value)
            }
            _ => Err("expected an expression".to_string()),
        }
    }
}

/// Evaluates one condition against a scalar lookup.
pub(crate) fn evaluate<F>(condition: &str, lookup: F) -> Result<bool, String>
where
    F: Fn(&str) -> Result<value::Scalar, String>,
{
    let mut parser = Parser::new(condition, lookup)?;
    Ok(truthy(&parser.parse()?))
}

/// Validates the condition syntax without resolving frame locals.
pub(crate) fn validate(condition: &str) -> Result<(), String> {
    evaluate(condition, |_| Ok(value::Scalar::Int(0))).map(|_| ())
}

/// PHP truthiness for the scalar subset.
fn truthy(value: &value::Scalar) -> bool {
    match value {
        value::Scalar::Int(value) => *value != 0,
        value::Scalar::Float(value) => *value != 0.0,
        value::Scalar::Bool(value) => *value,
        value::Scalar::Null => false,
        value::Scalar::String(value) => !value.is_empty() && value != "0",
        value::Scalar::Other => true,
    }
}

/// Numeric view used by arithmetic and ordering.
fn numeric(value: &value::Scalar) -> Result<f64, String> {
    match value {
        value::Scalar::Int(value) => Ok(*value as f64),
        value::Scalar::Float(value) => Ok(*value),
        value::Scalar::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
        value::Scalar::Null => Ok(0.0),
        value::Scalar::String(value) => value
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("non-numeric string '{value}' in condition")),
        value::Scalar::Other => Err("non-scalar value in condition".to_string()),
    }
}

/// Strict `===` over the scalar subset.
fn strict_eq(left: &value::Scalar, right: &value::Scalar) -> bool {
    match (left, right) {
        (value::Scalar::Int(left), value::Scalar::Int(right)) => left == right,
        (value::Scalar::Float(left), value::Scalar::Float(right)) => left == right,
        (value::Scalar::Bool(left), value::Scalar::Bool(right)) => left == right,
        (value::Scalar::String(left), value::Scalar::String(right)) => left == right,
        (value::Scalar::Null, value::Scalar::Null) => true,
        _ => false,
    }
}

/// Reduced PHP loose `==` over the scalar subset.
fn loose_eq(left: &value::Scalar, right: &value::Scalar) -> bool {
    if strict_eq(left, right) {
        return true;
    }
    match (left, right) {
        (value::Scalar::Bool(_), _) | (_, value::Scalar::Bool(_)) => {
            truthy(left) == truthy(right)
        }
        (value::Scalar::Null, _) | (_, value::Scalar::Null) => !truthy(left) || !truthy(right),
        (value::Scalar::Int(_) | value::Scalar::Float(_), _)
        | (_, value::Scalar::Int(_) | value::Scalar::Float(_)) => {
            numeric(left).ok() == numeric(right).ok()
        }
        (value::Scalar::String(left), value::Scalar::String(right)) => left == right,
        _ => false,
    }
}

/// Ordering over numbers and strings.
fn compare(left: &value::Scalar, right: &value::Scalar) -> Result<f64, String> {
    if let (value::Scalar::String(left), value::Scalar::String(right)) = (left, right) {
        return Ok(match left.cmp(right) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        });
    }
    Ok(numeric(left)? - numeric(right)?)
}

/// Addition with PHP's integer-preserving fast path.
fn add(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    if let (value::Scalar::Int(left), value::Scalar::Int(right)) = (left, right) {
        return Ok(value::Scalar::Int(left.wrapping_add(*right)));
    }
    Ok(value::Scalar::Float(numeric(left)? + numeric(right)?))
}

/// Subtraction with PHP's integer-preserving fast path.
fn subtract(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    if let (value::Scalar::Int(left), value::Scalar::Int(right)) = (left, right) {
        return Ok(value::Scalar::Int(left.wrapping_sub(*right)));
    }
    Ok(value::Scalar::Float(numeric(left)? - numeric(right)?))
}

/// Multiplication with PHP's integer-preserving fast path.
fn multiply(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    if let (value::Scalar::Int(left), value::Scalar::Int(right)) = (left, right) {
        return Ok(value::Scalar::Int(left.wrapping_mul(*right)));
    }
    Ok(value::Scalar::Float(numeric(left)? * numeric(right)?))
}

/// Division; division by zero and non-numeric strings are errors.
fn divide(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    let denominator = numeric(right)?;
    if denominator == 0.0 {
        return Err("division by zero in condition".to_string());
    }
    Ok(value::Scalar::Float(numeric(left)? / denominator))
}

/// Integer modulo; a zero divisor is an error.
fn modulo(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    let denominator = numeric(right)? as i64;
    if denominator == 0 {
        return Err("modulo by zero in condition".to_string());
    }
    Ok(value::Scalar::Int(numeric(left)? as i64 % denominator))
}

/// Unary negation.
fn negate(value: &value::Scalar) -> Result<value::Scalar, String> {
    match value {
        value::Scalar::Int(value) => Ok(value::Scalar::Int(value.wrapping_neg())),
        _ => Ok(value::Scalar::Float(-numeric(value)?)),
    }
}

impl Debugger {
    /// Evaluates a conditional breakpoint against the current activation.
    pub(crate) fn evaluate_condition(
        &self,
        condition: &str,
        frame: usize,
    ) -> Result<bool, String> {
        let Some(activation) = self.activation(0) else {
            return Err("no active frame".to_string());
        };
        if activation.frame != frame {
            return Err("condition evaluated against a stale frame".to_string());
        }
        let locals = self
            .locals
            .get(&(activation.function_id as u64))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        evaluate(condition, |name| {
            let local = locals
                .iter()
                .find(|local| local.name == name)
                .ok_or_else(|| format!("unknown local '${name}'"))?;
            Ok(value::scalar_value(value::local_value(local, frame)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a lookup over the supplied scalar map.
    fn lookup<'a>(
        entries: &'a [(&'a str, value::Scalar)],
    ) -> impl Fn(&str) -> Result<value::Scalar, String> + 'a {
        move |name| {
            entries
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| format!("unknown local '${name}'"))
        }
    }

    /// Verifies strict integer comparisons and boolean composition.
    #[test]
    fn evaluates_integer_conditions() {
        let entries = [("i", value::Scalar::Int(3))];
        let lookup = lookup(&entries);
        assert!(evaluate("$i === 3", &lookup).unwrap());
        assert!(!evaluate("$i === 4", &lookup).unwrap());
        assert!(evaluate("$i >= 3 && $i < 4", &lookup).unwrap());
        assert!(evaluate("($i + 1) * 2 == 8", &lookup).unwrap());
        assert!(!evaluate("!($i == 3)", &lookup).unwrap());
    }

    /// Verifies strings, booleans, null and loose numeric comparison.
    #[test]
    fn evaluates_mixed_scalar_conditions() {
        let entries = [
            ("name", value::Scalar::String("world".to_string())),
            ("flag", value::Scalar::Bool(true)),
            ("missing", value::Scalar::Null),
        ];
        let lookup = lookup(&entries);
        assert!(evaluate("$name === \"world\"", &lookup).unwrap());
        assert!(evaluate("$name == 'world'", &lookup).unwrap());
        assert!(!evaluate("!$flag", &lookup).unwrap());
        assert!(evaluate("$missing === null", &lookup).unwrap());
        assert!(evaluate("3 == 3.0", &lookup).unwrap());
    }

    /// Verifies malformed expressions and unknown locals are reported.
    #[test]
    fn rejects_malformed_and_unknown_conditions() {
        let lookup = lookup(&[]);
        assert!(evaluate("$i ===", &lookup).is_err());
        assert!(evaluate("$i & 3", &lookup).is_err());
        assert!(evaluate("$nope === 1", &lookup).is_err());
        assert!(validate("$i === 3").is_ok());
        assert!(validate("$i ===").is_err());
    }
}
