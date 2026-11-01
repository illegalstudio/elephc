//! Purpose:
//! Parses global/static declarations and throw/try/catch statements.
//!
//! Called from:
//! - Top-level and nested statement dispatch.
//!
//! Key details:
//! - Catch union types are canonicalized when parsed into EvalIR.

use super::*;
use crate::eval_ir::EvalDeclareValue;

impl Parser {
    /// Parses every `declare(…)` form php accepts, in both the statement and the block shapes.
    ///
    /// `declare` was in the reserved-word list with no statement behind it, so the whole directive
    /// parsed as a CALL and died on the `=` inside it. `strict_types` was then accepted alone and
    /// everything else refused -- but php REFUSES almost nothing here: an unknown directive is a
    /// warning and the script runs on, and `ticks` and `encoding` are ordinary accepted
    /// directives. Refusing them made a file php parses unparseable.
    ///
    /// Measured with `php -n` 8.5.6:
    /// - `declare(foo=1);` warns `Unsupported declare 'foo'` and CONTINUES;
    /// - `declare(encoding='UTF-8');` warns that it is ignored with Zend multibyte off;
    /// - `declare(strict_types=1) { … }` is a fatal: strict types has no block form;
    /// - `declare(ticks=1) { … }` and `declare(ticks=1): … enddeclare;` scope to their body.
    pub(in crate::parser) fn parse_declare_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let TokenKind::Ident(directive) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let directive = directive.clone();
        self.advance();
        self.expect(TokenKind::Equal)?;
        let value = self.parse_declare_directive_value()?;
        self.expect(TokenKind::RParen)?;
        let body = self.parse_declare_body()?;
        if ident_eq(&directive, "strict_types") {
            if body.is_some() {
                // php: `Fatal error: strict_types declaration must not use block mode`.
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let EvalDeclareValue::Int(value) = value else {
                return Err(EvalParseError::UnexpectedToken);
            };
            return Ok(vec![EvalStmt::DeclareStrictTypes(value != 0)]);
        }
        if ident_eq(&directive, "ticks") {
            let EvalDeclareValue::Int(value) = value else {
                return Err(EvalParseError::UnexpectedToken);
            };
            return Ok(vec![EvalStmt::DeclareTicks {
                every: value,
                body,
            }]);
        }
        Ok(vec![EvalStmt::DeclareDirective {
            name: directive,
            body,
        }])
    }

    /// Reads the value on the right of a `declare` directive.
    ///
    /// `ticks` and `strict_types` take an integer; `encoding` takes a string. Nothing else is
    /// reached, because an unknown directive only ever has its NAME reported.
    fn parse_declare_directive_value(&mut self) -> Result<EvalDeclareValue, EvalParseError> {
        match self.current() {
            TokenKind::Int(value) => {
                let value = *value;
                self.advance();
                Ok(EvalDeclareValue::Int(value))
            }
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance();
                Ok(EvalDeclareValue::Str(value))
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Reads a `declare` body: `;` for the rest of the scope, or a block that scopes the directive.
    fn parse_declare_body(&mut self) -> Result<Option<Vec<EvalStmt>>, EvalParseError> {
        if self.consume(TokenKind::Semicolon) {
            return Ok(None);
        }
        if matches!(self.current(), TokenKind::LBrace | TokenKind::Colon) {
            return Ok(Some(self.parse_statement_body_or_alternative("enddeclare")?));
        }
        // A closing `?>` ends the statement exactly like a semicolon.
        self.expect_semicolon()?;
        Ok(None)
    }

    /// Parses `global $name, $other;` declarations in eval fragments.
    pub(in crate::parser) fn parse_global_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let mut vars = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            vars.push(name.clone());
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Global { vars }])
    }

    /// Parses `static $name = expr;` or `static $name;` declarations in eval fragments.
    /// A missing initializer desugars to `= null`, matching PHP semantics.
    pub(in crate::parser) fn parse_static_var_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        let init = if self.consume(TokenKind::Equal) {
            self.parse_expr()?
        } else {
            EvalExpr::Const(EvalConst::Null)
        };
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::StaticVar { name, init }])
    }

    /// Parses `throw expr;` statements in eval fragments.
    pub(in crate::parser) fn parse_throw_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let expr = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Throw(expr)])
    }

    /// Parses `try { ... } catch (Type|Other $name) { ... } finally { ... }` statements.
    pub(in crate::parser) fn parse_try_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let body = self.parse_block()?;
        let mut catches = Vec::new();
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "catch")) {
            catches.push(self.parse_catch_clause()?);
        }
        let has_finally = matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "finally"));
        let finally_body = if has_finally {
            self.advance();
            self.parse_block()?
        } else {
            Vec::new()
        };
        if catches.is_empty() && !has_finally {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(vec![EvalStmt::Try {
            body,
            catches,
            finally_body,
        }])
    }

    /// Parses one `catch (ClassName|Other [$name]) { ... }` clause.
    pub(in crate::parser) fn parse_catch_clause(&mut self) -> Result<EvalCatch, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let class_names = self.parse_catch_types()?;
        let var_name = if let TokenKind::DollarIdent(var_name) = self.current() {
            let var_name = var_name.clone();
            self.advance();
            Some(var_name)
        } else {
            None
        };
        self.expect(TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(EvalCatch {
            class_names,
            var_name,
            body,
        })
    }

    /// Parses one or more unioned catch types in source order.
    pub(in crate::parser) fn parse_catch_types(&mut self) -> Result<Vec<String>, EvalParseError> {
        let class_name = self.parse_class_reference_name(false)?;
        let mut class_names = vec![self.resolve_class_name(class_name)];
        while self.consume(TokenKind::Pipe) {
            let class_name = self.parse_class_reference_name(false)?;
            class_names.push(self.resolve_class_name(class_name));
        }
        Ok(class_names)
    }
}
