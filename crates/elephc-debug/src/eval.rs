//! Purpose:
//! Read-only PHP expression evaluation for DAP `evaluate`, hovers and watch
//! expressions. The evaluator reads frame locals, object properties, array/hash
//! entries and a small allowlist of scalar builtins without executing PHP code.
//!
//! Called from:
//! - `crate::dap::Debugger::dap_evaluate`.
//!
//! Key details:
//! - The expression language is deliberately smaller than PHP: assignments,
//!   user calls, `new`, magic accessors and `ArrayAccess` are refused.
//! - Borrowed values keep their runtime storage for child expansion; computed
//!   scalars are rendered from the shared `value::Scalar` contract.
//! - Property access never runs a getter: virtual properties are errors and a
//!   backed getter is returned from its raw slot with an explicit note.

use std::collections::HashMap;

use super::{value, Local};

/// One successful evaluation result.
pub(crate) enum EvalValue {
    /// A live runtime value that can be expanded through `variablesReference`.
    Borrowed {
        /// Borrowed value from the stopped frame.
        value: value::Value,
        /// Read-only caveat appended to the rendered result.
        note: Option<String>,
    },
    /// A computed scalar with no runtime storage.
    Scalar(value::Scalar),
}

/// Evaluates one read-only expression against a stopped frame.
pub(crate) fn evaluate(
    expression: &str,
    locals: &[Local],
    frame: usize,
    classes: &HashMap<u64, value::Class>,
) -> Result<EvalValue, String> {
    let tokens = tokenize(expression)?;
    let mut parser = Parser {
        tokens,
        position: 0,
        locals,
        frame,
        classes,
    };
    let expression = parser.parse_expression()?;
    parser.expect_end()?;
    parser.eval(&expression)
}

/// One token in the read-only expression language.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Int(i64),
    Float(f64),
    String(String),
    Var(String),
    Ident(String),
    Arrow,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Dot,
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
    Not,
    End,
}

/// One parsed expression node.
enum Expr {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    Var(String),
    Property(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
}

/// Supported binary operators.
#[derive(Clone, Copy)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
    Eq,
    StrictEq,
    NotEq,
    StrictNotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// Supported unary operators.
#[derive(Clone, Copy)]
enum UnaryOp {
    Not,
    Neg,
    Pos,
}

/// Tokenizes one expression.
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
                "true" | "TRUE" | "True" => Token::Ident("true".to_string()),
                "false" | "FALSE" | "False" => Token::Ident("false".to_string()),
                "null" | "NULL" | "Null" => Token::Ident("null".to_string()),
                _ => Token::Ident(word),
            });
            continue;
        }
        if byte.is_ascii_digit() {
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
                tokens.push(Token::Float(
                    text.parse::<f64>()
                        .map_err(|_| format!("invalid float literal '{text}'"))?,
                ));
            } else {
                tokens.push(Token::Int(
                    text.parse::<i64>()
                        .map_err(|_| format!("integer literal out of range: '{text}'"))?,
                ));
            }
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            let quote = byte;
            index += 1;
            let mut text = Vec::new();
            let mut closed = false;
            while index < bytes.len() {
                let current = bytes[index];
                if current == quote {
                    index += 1;
                    closed = true;
                    break;
                }
                if current == b'\\' && index + 1 < bytes.len() {
                    let escaped = bytes[index + 1];
                    index += 2;
                    match (quote, escaped) {
                        (b'\'', b'\\') | (b'"', b'\\') => text.push(b'\\'),
                        (b'\'', b'\'') | (b'"', b'"') => text.push(quote),
                        (b'"', b'n') => text.push(b'\n'),
                        (b'"', b'r') => text.push(b'\r'),
                        (b'"', b't') => text.push(b'\t'),
                        (b'"', b'0') => text.push(b'\0'),
                        (b'"', b'$') => text.push(b'$'),
                        (_, other) => {
                            text.push(b'\\');
                            text.push(other);
                        }
                    }
                    continue;
                }
                text.push(current);
                index += 1;
            }
            if !closed {
                return Err("unterminated string literal".to_string());
            }
            tokens.push(Token::String(String::from_utf8_lossy(&text).into_owned()));
            continue;
        }
        match byte {
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                tokens.push(Token::Arrow);
                index += 2;
            }
            b'[' => {
                tokens.push(Token::LBracket);
                index += 1;
            }
            b']' => {
                tokens.push(Token::RBracket);
                index += 1;
            }
            b'(' => {
                tokens.push(Token::LParen);
                index += 1;
            }
            b')' => {
                tokens.push(Token::RParen);
                index += 1;
            }
            b',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            b'+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            b'-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            b'*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            b'/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            b'%' => {
                tokens.push(Token::Percent);
                index += 1;
            }
            b'.' => {
                tokens.push(Token::Dot);
                index += 1;
            }
            b'=' if bytes.get(index + 1) == Some(&b'=') => {
                if bytes.get(index + 2) == Some(&b'=') {
                    tokens.push(Token::EqEqEq);
                    index += 3;
                } else {
                    tokens.push(Token::EqEq);
                    index += 2;
                }
            }
            b'=' => return Err("assignment is not supported in the read-only evaluator".to_string()),
            b'!' if bytes.get(index + 1) == Some(&b'=') => {
                if bytes.get(index + 2) == Some(&b'=') {
                    tokens.push(Token::NotEqEq);
                    index += 3;
                } else {
                    tokens.push(Token::NotEq);
                    index += 2;
                }
            }
            b'!' => {
                tokens.push(Token::Not);
                index += 1;
            }
            b'<' if bytes.get(index + 1) == Some(&b'=') => {
                tokens.push(Token::Le);
                index += 2;
            }
            b'<' => {
                tokens.push(Token::Lt);
                index += 1;
            }
            b'>' if bytes.get(index + 1) == Some(&b'=') => {
                tokens.push(Token::Ge);
                index += 2;
            }
            b'>' => {
                tokens.push(Token::Gt);
                index += 1;
            }
            b'&' if bytes.get(index + 1) == Some(&b'&') => {
                tokens.push(Token::AndAnd);
                index += 2;
            }
            b'|' if bytes.get(index + 1) == Some(&b'|') => {
                tokens.push(Token::OrOr);
                index += 2;
            }
            _ => {
                return Err(format!(
                    "unexpected character '{}' in expression",
                    byte as char
                ))
            }
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

/// Recursive-descent parser and evaluator over one stopped frame.
struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    locals: &'a [Local],
    frame: usize,
    classes: &'a HashMap<u64, value::Class>,
}

impl<'a> Parser<'a> {
    /// Returns the current token without consuming it.
    fn peek(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::End)
    }

    /// Consumes and returns the current token.
    fn next(&mut self) -> Token {
        let token = self.tokens.get(self.position).cloned().unwrap_or(Token::End);
        self.position += 1;
        token
    }

    /// Consumes the expected token or reports a syntax error.
    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let token = self.next();
        if &token == expected {
            Ok(())
        } else {
            Err(format!("expected {expected:?}, found {token:?}"))
        }
    }

    /// Requires the end of the expression.
    fn expect_end(&self) -> Result<(), String> {
        if matches!(self.peek(), Token::End) {
            Ok(())
        } else {
            Err(format!("unexpected token {:?}", self.peek()))
        }
    }

    /// Parses one expression with PHP-like precedence.
    fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    /// Parses `||`.
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Token::OrOr) {
            self.next();
            let right = self.parse_and()?;
            left = Expr::Binary(BinaryOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses `&&`.
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality()?;
        while matches!(self.peek(), Token::AndAnd) {
            self.next();
            let right = self.parse_equality()?;
            left = Expr::Binary(BinaryOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses equality operators.
    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_relational()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinaryOp::Eq,
                Token::EqEqEq => BinaryOp::StrictEq,
                Token::NotEq => BinaryOp::NotEq,
                Token::NotEqEq => BinaryOp::StrictNotEq,
                _ => break,
            };
            self.next();
            let right = self.parse_relational()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses relational operators.
    fn parse_relational(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinaryOp::Lt,
                Token::Le => BinaryOp::Le,
                Token::Gt => BinaryOp::Gt,
                Token::Ge => BinaryOp::Ge,
                _ => break,
            };
            self.next();
            let right = self.parse_additive()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses `+`, `-`, `.`.
    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                Token::Dot => BinaryOp::Concat,
                _ => break,
            };
            self.next();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses `*`, `/`, `%`.
    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.next();
            let right = self.parse_unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses unary operators.
    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Token::Not => {
                self.next();
                Ok(Expr::Unary(UnaryOp::Not, Box::new(self.parse_unary()?)))
            }
            Token::Minus => {
                self.next();
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(self.parse_unary()?)))
            }
            Token::Plus => {
                self.next();
                Ok(Expr::Unary(UnaryOp::Pos, Box::new(self.parse_unary()?)))
            }
            _ => self.parse_postfix(),
        }
    }

    /// Parses property and index postfix operations.
    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_primary()?;
        loop {
            match self.peek() {
                Token::Arrow => {
                    self.next();
                    let Token::Ident(name) = self.next() else {
                        return Err("expected a property name after '->'".to_string());
                    };
                    expression = Expr::Property(Box::new(expression), name);
                }
                Token::LBracket => {
                    self.next();
                    let index = self.parse_expression()?;
                    self.expect(&Token::RBracket)?;
                    expression = Expr::Index(Box::new(expression), Box::new(index));
                }
                _ => break,
            }
        }
        Ok(expression)
    }

    /// Parses literals, locals, calls and parenthesized expressions.
    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Token::Int(value) => Ok(Expr::Int(value)),
            Token::Float(value) => Ok(Expr::Float(value)),
            Token::String(value) => Ok(Expr::String(value)),
            Token::Var(name) => Ok(Expr::Var(name)),
            Token::Ident(name) => match name.as_str() {
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                "null" => Ok(Expr::Null),
                _ if matches!(self.peek(), Token::LParen) => {
                    self.next();
                    let mut arguments = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        loop {
                            arguments.push(self.parse_expression()?);
                            if matches!(self.peek(), Token::Comma) {
                                self.next();
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::Call(name, arguments))
                }
                _ => Err(format!("unsupported name '{name}' in expression")),
            },
            Token::LParen => {
                let expression = self.parse_expression()?;
                self.expect(&Token::RParen)?;
                Ok(expression)
            }
            token => Err(format!("unexpected token {token:?} in expression")),
        }
    }

    /// Evaluates one parsed expression.
    fn eval(&self, expression: &Expr) -> Result<EvalValue, String> {
        match expression {
            Expr::Int(value) => Ok(EvalValue::Scalar(value::Scalar::Int(*value))),
            Expr::Float(value) => Ok(EvalValue::Scalar(value::Scalar::Float(*value))),
            Expr::String(value) => Ok(EvalValue::Scalar(value::Scalar::String(value.clone()))),
            Expr::Bool(value) => Ok(EvalValue::Scalar(value::Scalar::Bool(*value))),
            Expr::Null => Ok(EvalValue::Scalar(value::Scalar::Null)),
            Expr::Var(name) => {
                let local = self
                    .locals
                    .iter()
                    .find(|local| local.name == *name)
                    .ok_or_else(|| format!("unknown local '${name}'"))?;
                Ok(EvalValue::Borrowed {
                    value: value::local_value(local, self.frame),
                    note: None,
                })
            }
            Expr::Property(base, name) => {
                let base = self.eval(base)?;
                let base = self.borrowed(base, "property access")?;
                self.property(base, name)
            }
            Expr::Index(base, index) => {
                let base = self.eval(base)?;
                let base = self.borrowed(base, "array access")?;
                let index = self.eval(index)?;
                let index = self.scalar(index, "array key")?;
                if base.tag == value::TAG_OBJECT {
                    return Err(
                        "array access on an object would execute ArrayAccess; not supported"
                            .to_string(),
                    );
                }
                let value = value::element_at(base, &index)
                    .ok_or_else(|| format!("undefined array key {}", key_label(&index)))?;
                Ok(EvalValue::Borrowed { value, note: None })
            }
            Expr::Call(name, arguments) => self.call(name, arguments),
            Expr::Binary(op, left, right) => self.binary(*op, left, right),
            Expr::Unary(op, operand) => {
                let operand = self.eval(operand)?;
                match op {
                    UnaryOp::Not => Ok(EvalValue::Scalar(value::Scalar::Bool(!truthy(
                        &self.scalar(operand, "logical negation")?,
                    )))),
                    UnaryOp::Neg => {
                        let value = self.scalar(operand, "unary negation")?;
                        Ok(EvalValue::Scalar(negate(&value)?))
                    }
                    UnaryOp::Pos => {
                        let value = self.scalar(operand, "unary plus")?;
                        Ok(EvalValue::Scalar(positive(&value)?))
                    }
                }
            }
        }
    }

    /// Evaluates one allowlisted builtin call.
    fn call(&self, name: &str, arguments: &[Expr]) -> Result<EvalValue, String> {
        let lower = name.to_ascii_lowercase();
        if !matches!(
            lower.as_str(),
            "count"
                | "strlen"
                | "gettype"
                | "is_null"
                | "is_int"
                | "is_string"
                | "is_array"
                | "is_object"
                | "is_bool"
                | "is_float"
        ) {
            return Err(format!(
                "unsupported function '{name}()' in the read-only evaluator"
            ));
        }
        if arguments.len() != 1 {
            return Err(format!("{name}() expects exactly one argument"));
        }
        let argument = self.eval(&arguments[0])?;
        match lower.as_str() {
            "count" => {
                let value = self.borrowed(argument, "count()")?;
                value::count(value).map(|count| EvalValue::Scalar(value::Scalar::Int(count as i64)))
            }
            "strlen" => {
                let value = self.borrowed(argument, "strlen()")?;
                value::strlen(value).map(|length| EvalValue::Scalar(value::Scalar::Int(length as i64)))
            }
            "gettype" => {
                let name = self.type_name(&argument);
                Ok(EvalValue::Scalar(value::Scalar::String(name.to_string())))
            }
            "is_null" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "NULL",
            ))),
            "is_int" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "integer",
            ))),
            "is_string" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "string",
            ))),
            "is_array" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "array",
            ))),
            "is_object" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "object",
            ))),
            "is_bool" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "boolean",
            ))),
            "is_float" => Ok(EvalValue::Scalar(value::Scalar::Bool(
                self.type_name(&argument) == "double",
            ))),
            _ => unreachable!("allowlist was checked"),
        }
    }

    /// Evaluates one binary operation with PHP-like short-circuiting.
    fn binary(&self, op: BinaryOp, left: &Expr, right: &Expr) -> Result<EvalValue, String> {
        if matches!(op, BinaryOp::And) {
            let left = self.eval(left)?;
            if !truthy(&self.scalar(left, "&&")?) {
                return Ok(EvalValue::Scalar(value::Scalar::Bool(false)));
            }
            let right = self.eval(right)?;
            return Ok(EvalValue::Scalar(value::Scalar::Bool(truthy(&self.scalar(
                right, "&&",
            )?))));
        }
        if matches!(op, BinaryOp::Or) {
            let left = self.eval(left)?;
            if truthy(&self.scalar(left, "||")?) {
                return Ok(EvalValue::Scalar(value::Scalar::Bool(true)));
            }
            let right = self.eval(right)?;
            return Ok(EvalValue::Scalar(value::Scalar::Bool(truthy(&self.scalar(
                right, "||",
            )?))));
        }
        let left = self.scalar(self.eval(left)?, "binary operation")?;
        let right = self.scalar(self.eval(right)?, "binary operation")?;
        let result = match op {
            BinaryOp::Add => add(&left, &right)?,
            BinaryOp::Sub => subtract(&left, &right)?,
            BinaryOp::Mul => multiply(&left, &right)?,
            BinaryOp::Div => divide(&left, &right)?,
            BinaryOp::Mod => modulo(&left, &right)?,
            BinaryOp::Concat => value::Scalar::String(format!(
                "{}{}",
                to_php_string(&left)?,
                to_php_string(&right)?
            )),
            BinaryOp::Eq => value::Scalar::Bool(loose_eq(&left, &right)),
            BinaryOp::StrictEq => value::Scalar::Bool(strict_eq(&left, &right)),
            BinaryOp::NotEq => value::Scalar::Bool(!loose_eq(&left, &right)),
            BinaryOp::StrictNotEq => value::Scalar::Bool(!strict_eq(&left, &right)),
            BinaryOp::Lt => value::Scalar::Bool(compare(&left, &right)? < 0.0),
            BinaryOp::Le => value::Scalar::Bool(compare(&left, &right)? <= 0.0),
            BinaryOp::Gt => value::Scalar::Bool(compare(&left, &right)? > 0.0),
            BinaryOp::Ge => value::Scalar::Bool(compare(&left, &right)? >= 0.0),
            BinaryOp::And | BinaryOp::Or => unreachable!("short-circuited above"),
        };
        Ok(EvalValue::Scalar(result))
    }

    /// Resolves one object property without executing hooks or magic accessors.
    fn property(&self, base: value::Value, name: &str) -> Result<EvalValue, String> {
        let base = value::unbox_value(base)
            .ok_or_else(|| "cannot read a property of this value".to_string())?;
        if base.tag != value::TAG_OBJECT {
            return Err(format!("cannot read property '{name}' of a non-object"));
        }
        let class_id = value::object_class_id(base.lo)
            .ok_or_else(|| "object class metadata is unavailable".to_string())?;
        let class = self
            .classes
            .get(&class_id)
            .ok_or_else(|| "object class metadata is unavailable".to_string())?;
        if let Some(property) = class.properties.iter().find(|property| property.name == name) {
            let note = if let Some(hooks) = class.property_hooks.get(name) {
                if hooks.is_virtual {
                    return Err(format!(
                        "property '{name}' is virtual; reading it would execute its get hook"
                    ));
                }
                if hooks.requires_get() {
                    Some(
                        value::PropertyHookNote {
                            hooks: *hooks,
                            php_type: property.php_type.clone(),
                        }
                        .backing_suffix()
                        .to_string(),
                    )
                } else {
                    None
                }
            } else {
                None
            };
            let value = value::object_property_value(base.lo, class_id, property)
                .ok_or_else(|| format!("property '{name}' is not readable"))?;
            return Ok(EvalValue::Borrowed { value, note });
        }
        if let Some(offset) = class.dynamic_offset {
            if let Some(value) =
                value::object_dynamic_property_value(base.lo, class_id, offset, name.as_bytes())
            {
                return Ok(EvalValue::Borrowed { value, note: None });
            }
        }
        Err(format!("property '{name}' does not exist on {}", class.name))
    }

    /// Requires an evaluation result to be a borrowed runtime value.
    fn borrowed(&self, value: EvalValue, context: &str) -> Result<value::Value, String> {
        match value {
            EvalValue::Borrowed { value, .. } => Ok(value),
            EvalValue::Scalar(_) => Err(format!("{context} requires a non-scalar value")),
        }
    }

    /// Requires an evaluation result to be scalar.
    fn scalar(&self, value: EvalValue, context: &str) -> Result<value::Scalar, String> {
        match value {
            EvalValue::Scalar(value) => Ok(value),
            EvalValue::Borrowed { value, .. } => match value::scalar_value(value) {
                value::Scalar::Other => Err(format!("{context} requires a scalar value")),
                scalar => Ok(scalar),
            },
        }
    }

    /// Returns the PHP type spelling for one evaluation result.
    fn type_name(&self, value: &EvalValue) -> &'static str {
        match value {
            EvalValue::Scalar(scalar) => scalar_type_name(scalar),
            EvalValue::Borrowed { value, .. } => value::gettype(*value),
        }
    }
}

/// Returns the PHP type spelling for one computed scalar.
fn scalar_type_name(value: &value::Scalar) -> &'static str {
    match value {
        value::Scalar::Int(_) => "integer",
        value::Scalar::Float(_) => "double",
        value::Scalar::Bool(_) => "boolean",
        value::Scalar::Null => "NULL",
        value::Scalar::String(_) => "string",
        value::Scalar::Other => "unknown type",
    }
}

/// Formats one key for an error message.
fn key_label(key: &value::Scalar) -> String {
    match key {
        value::Scalar::Int(value) => value.to_string(),
        value::Scalar::String(value) => format!("\"{value}\""),
        _ => "<key>".to_string(),
    }
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
            .map_err(|_| format!("non-numeric string '{value}' in expression")),
        value::Scalar::Other => Err("non-scalar value in expression".to_string()),
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
        (value::Scalar::Null, other) | (other, value::Scalar::Null) => match other {
            value::Scalar::Bool(value) => !*value,
            value::Scalar::Int(value) => *value == 0,
            value::Scalar::Float(value) => *value == 0.0,
            value::Scalar::String(value) => value.is_empty(),
            _ => false,
        },
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
        return Ok(match left.checked_add(*right) {
            Some(value) => value::Scalar::Int(value),
            None => value::Scalar::Float(*left as f64 + *right as f64),
        });
    }
    Ok(value::Scalar::Float(numeric(left)? + numeric(right)?))
}

/// Subtraction with PHP's integer-preserving fast path.
fn subtract(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    if let (value::Scalar::Int(left), value::Scalar::Int(right)) = (left, right) {
        return Ok(match left.checked_sub(*right) {
            Some(value) => value::Scalar::Int(value),
            None => value::Scalar::Float(*left as f64 - *right as f64),
        });
    }
    Ok(value::Scalar::Float(numeric(left)? - numeric(right)?))
}

/// Multiplication with PHP's integer-preserving fast path.
fn multiply(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    if let (value::Scalar::Int(left), value::Scalar::Int(right)) = (left, right) {
        return Ok(match left.checked_mul(*right) {
            Some(value) => value::Scalar::Int(value),
            None => value::Scalar::Float(*left as f64 * *right as f64),
        });
    }
    Ok(value::Scalar::Float(numeric(left)? * numeric(right)?))
}

/// Division; division by zero and non-numeric strings are errors.
fn divide(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    if let (value::Scalar::Int(left), value::Scalar::Int(right)) = (left, right) {
        if *right != 0 {
            if let (Some(quotient), Some(remainder)) =
                (left.checked_div(*right), left.checked_rem(*right))
            {
                if remainder == 0 {
                    return Ok(value::Scalar::Int(quotient));
                }
            }
        }
    }
    let denominator = numeric(right)?;
    if denominator == 0.0 {
        return Err("division by zero in expression".to_string());
    }
    Ok(value::Scalar::Float(numeric(left)? / denominator))
}

/// Integer modulo; a zero divisor is an error.
fn modulo(left: &value::Scalar, right: &value::Scalar) -> Result<value::Scalar, String> {
    let denominator = numeric(right)? as i64;
    if denominator == 0 {
        return Err("modulo by zero in expression".to_string());
    }
    let numerator = numeric(left)? as i64;
    Ok(value::Scalar::Int(
        numerator.checked_rem(denominator).unwrap_or(0),
    ))
}

/// Unary negation.
fn negate(value: &value::Scalar) -> Result<value::Scalar, String> {
    match value {
        value::Scalar::Int(value) => Ok(match value.checked_neg() {
            Some(value) => value::Scalar::Int(value),
            None => value::Scalar::Float(-(*value as f64)),
        }),
        _ => Ok(value::Scalar::Float(-numeric(value)?)),
    }
}

/// Unary plus.
fn positive(value: &value::Scalar) -> Result<value::Scalar, String> {
    match value {
        value::Scalar::Int(_) | value::Scalar::Float(_) => Ok(value.clone()),
        _ => Ok(value::Scalar::Float(numeric(value)?)),
    }
}

/// Converts one scalar to PHP's string representation.
fn to_php_string(value: &value::Scalar) -> Result<String, String> {
    match value {
        value::Scalar::Int(value) => Ok(value.to_string()),
        value::Scalar::Float(value) => Ok(format!("{value}")),
        value::Scalar::Bool(value) => Ok(if *value { "1".to_string() } else { String::new() }),
        value::Scalar::Null => Ok(String::new()),
        value::Scalar::String(value) => Ok(value.clone()),
        value::Scalar::Other => Err("non-scalar value in string conversion".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the evaluator preserves arithmetic precedence.
    #[test]
    fn evaluates_arithmetic_precedence() {
        let classes = HashMap::new();
        match evaluate("1 + 2 * 3", &[], 0, &classes).expect("expression") {
            EvalValue::Scalar(value::Scalar::Int(value)) => assert_eq!(value, 7),
            _ => panic!("expected an integer result"),
        }
    }

    /// Verifies assignments and unknown names fail closed.
    #[test]
    fn rejects_assignment_and_unknown_calls() {
        let classes = HashMap::new();
        assert!(evaluate("$x = 1", &[], 0, &classes).is_err());
        assert!(evaluate("mystery(1)", &[], 0, &classes).is_err());
        assert!(evaluate("$missing", &[], 0, &classes).is_err());
    }

    /// Verifies integer overflow in division/remainder cannot abort a watch.
    #[test]
    fn division_and_remainder_handle_integer_overflow() {
        let min = value::Scalar::Int(i64::MIN);
        let minus_one = value::Scalar::Int(-1);
        assert!(matches!(
            divide(&min, &minus_one).expect("division result"),
            value::Scalar::Float(_)
        ));
        assert!(matches!(
            modulo(&min, &minus_one).expect("remainder result"),
            value::Scalar::Int(0)
        ));
    }

    /// Verifies UTF-8 string literals survive tokenization unchanged.
    #[test]
    fn preserves_non_ascii_string_literals() {
        let classes = HashMap::new();
        match evaluate("'café' === 'café'", &[], 0, &classes).expect("utf8 string") {
            EvalValue::Scalar(value::Scalar::Bool(value)) => assert!(value),
            _ => panic!("expected a boolean result"),
        }
    }

    /// Verifies PHP's null loose-comparison rules for scalars.
    #[test]
    fn null_uses_php_loose_comparison_rules() {
        let null = value::Scalar::Null;
        assert!(loose_eq(&null, &value::Scalar::Int(0)));
        assert!(loose_eq(&value::Scalar::String(String::new()), &null));
        assert!(!loose_eq(&null, &value::Scalar::String("0".to_string())));
        assert!(!loose_eq(&null, &value::Scalar::Int(5)));
    }

    /// Verifies short-circuit operators do not evaluate the right side.
    #[test]
    fn short_circuits_boolean_operators() {
        let classes = HashMap::new();
        match evaluate("false && mystery(1)", &[], 0, &classes).expect("short circuit") {
            EvalValue::Scalar(value::Scalar::Bool(value)) => assert!(!value),
            _ => panic!("expected a boolean result"),
        }
        match evaluate("true || mystery(1)", &[], 0, &classes).expect("short circuit") {
            EvalValue::Scalar(value::Scalar::Bool(value)) => assert!(value),
            _ => panic!("expected a boolean result"),
        }
    }
}
