//! Purpose:
//! Parses call arguments, first-class callable markers, closures/captures, and
//! modern or legacy array literals.
//!
//! Called from:
//! - Primary, postfix, static-member, and object-construction expression parsing.
//!
//! Key details:
//! - Source-order arguments, spread/named syntax, and closure captures remain intact.

use super::*;

impl Parser {

    /// Parses a parenthesized source-order argument list.
    pub(in crate::parser) fn parse_call_args(&mut self) -> Result<Vec<EvalCallArg>, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_call_arg()?);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RParen) {
                return Ok(args);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    /// Parses one positional or named argument within a call argument list.
    pub(in crate::parser) fn parse_call_arg(&mut self) -> Result<EvalCallArg, EvalParseError> {
        if self.consume(TokenKind::Ellipsis) {
            return self.parse_expr().map(EvalCallArg::spread);
        }
        if matches!(self.peek(), TokenKind::Colon) {
            if let TokenKind::Ident(name) = self.current() {
                let name = name.clone();
                self.advance();
                self.expect(TokenKind::Colon)?;
                let value = self.parse_expr()?;
                return Ok(EvalCallArg::named(name, value));
            }
        }
        self.parse_expr().map(EvalCallArg::positional)
    }

    /// Consumes PHP's `(...)` first-class callable marker when it is the whole argument list.
    pub(super) fn consume_first_class_callable_marker(&mut self) -> bool {
        if matches!(self.current(), TokenKind::LParen)
            && matches!(self.tokens.get(self.pos + 1), Some(TokenKind::Ellipsis))
            && matches!(self.tokens.get(self.pos + 2), Some(TokenKind::RParen))
        {
            self.advance();
            self.advance();
            self.advance();
            true
        } else {
            false
        }
    }

    /// Builds an eval function-callable expression with namespace fallback metadata.
    pub(super) fn function_callable_expr(&self, name: String) -> EvalExpr {
        if let Some(imported) = self.imports.resolve_function(&name) {
            return Self::function_callable_value(imported.to_ascii_lowercase(), None);
        }
        let fallback_name = name.to_ascii_lowercase();
        if self.namespace.is_empty() {
            Self::function_callable_value(fallback_name, None)
        } else {
            Self::function_callable_value(
                self.qualify_name_in_current_namespace(&name)
                    .to_ascii_lowercase(),
                Some(fallback_name),
            )
        }
    }

    /// Builds the EvalIR node that resolves a first-class function callable at runtime.
    pub(super) fn function_callable_value(name: String, fallback_name: Option<String>) -> EvalExpr {
        EvalExpr::FunctionCallable {
            name,
            fallback_name,
        }
    }

    /// Builds the EvalIR node used for object method first-class callables.
    pub(super) fn method_callable_expr(object: EvalExpr, method: EvalExpr) -> EvalExpr {
        EvalExpr::MethodCallable {
            object: Box::new(object),
            method: Box::new(method),
        }
    }

    /// Builds the EvalIR node used for invokable-object first-class callables.
    pub(super) fn invokable_callable_expr(object: EvalExpr) -> EvalExpr {
        EvalExpr::InvokableCallable {
            object: Box::new(object),
        }
    }

    /// Builds the EvalIR node used for runtime-class static first-class callables.
    pub(super) fn dynamic_static_method_callable_expr(class_name: EvalExpr, method: EvalExpr) -> EvalExpr {
        EvalExpr::DynamicStaticMethodCallable {
            class_name: Box::new(class_name),
            method: Box::new(method),
        }
    }

    /// Parses an anonymous function expression into a runtime eval closure payload.
    pub(super) fn parse_closure_expr(&mut self, is_static: bool) -> Result<EvalExpr, EvalParseError> {
        let source_start_line = self.current_line();
        if is_static {
            self.advance();
        }
        self.advance();
        self.expect(TokenKind::LParen)?;
        let ParsedMethodParams {
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            promoted_properties,
            promoted_assignments,
        } = self.parse_method_params("", false)?;
        if !promoted_properties.is_empty() || !promoted_assignments.is_empty() {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let captures = self.parse_optional_closure_use_captures(&params)?;
        let return_type = self.parse_optional_return_type(EvalTypePosition::FunctionReturn)?;
        let (body, source_end_line) = self.parse_block_with_end_line()?;
        let function = EvalFunction::new(next_closure_function_name(), params, body)
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
            .with_parameter_attributes(parameter_attributes)
            .with_parameter_types(parameter_types)
            .with_parameter_defaults(parameter_defaults)
            .with_parameter_by_ref_flags(parameter_is_by_ref)
            .with_parameter_variadic_flags(parameter_is_variadic)
            .with_return_type(return_type);
        Ok(EvalExpr::Closure {
            function,
            captures,
            is_static,
        })
    }

    /// Parses an optional closure `use (...)` capture list.
    pub(super) fn parse_optional_closure_use_captures(
        &mut self,
        params: &[String],
    ) -> Result<Vec<EvalClosureCapture>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "use")) {
            return Ok(Vec::new());
        }
        self.advance();
        self.expect(TokenKind::LParen)?;
        if self.consume(TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let mut captures = Vec::new();
        loop {
            let by_ref = self.consume(TokenKind::Ampersand);
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            if params.iter().any(|param| param == name)
                || captures
                    .iter()
                    .any(|capture: &EvalClosureCapture| capture.name() == name)
            {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            captures.push(EvalClosureCapture::new(name.clone(), by_ref));
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if matches!(self.current(), TokenKind::RParen) {
                return Err(EvalParseError::ExpectedVariable);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(captures)
    }

    /// Parses an array literal with source-order optional key/value element expressions.
    pub(in crate::parser) fn parse_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.expect(TokenKind::LBracket)?;
        self.parse_array_elements_until(TokenKind::RBracket)
    }

    /// Parses PHP's legacy `array(...)` literal into the same EvalIR node as `[...]`.
    pub(in crate::parser) fn parse_legacy_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        self.parse_array_elements_until(TokenKind::RParen)
    }

    /// Returns whether the current token starts PHP's legacy `array(...)` literal syntax.
    pub(in crate::parser) fn current_starts_legacy_array_literal(&self) -> bool {
        matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "array"))
            && matches!(self.peek(), TokenKind::LParen)
    }

    /// Parses comma-separated array elements until the supplied closing delimiter.
    pub(in crate::parser) fn parse_array_elements_until(
        &mut self,
        close: TokenKind,
    ) -> Result<EvalExpr, EvalParseError> {
        let mut elements = Vec::new();
        if self.consume(close.clone()) {
            return Ok(EvalExpr::Array(elements));
        }
        loop {
            if self.consume(TokenKind::Ampersand) {
                let value = self.parse_expr()?;
                elements.push(EvalArrayElement::Reference(value));
                if !self.consume(TokenKind::Comma) {
                    break;
                }
                if self.consume(close.clone()) {
                    return Ok(EvalExpr::Array(elements));
                }
                continue;
            }
            let first = self.parse_expr()?;
            if self.consume(TokenKind::FatArrow) {
                if self.consume(TokenKind::Ampersand) {
                    let value = self.parse_expr()?;
                    elements.push(EvalArrayElement::KeyReference {
                        key: first,
                        value,
                    });
                } else {
                    let value = self.parse_expr()?;
                    elements.push(EvalArrayElement::KeyValue { key: first, value });
                }
            } else {
                elements.push(EvalArrayElement::Value(first));
            }
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(close.clone()) {
                return Ok(EvalExpr::Array(elements));
            }
        }
        self.expect(close)?;
        Ok(EvalExpr::Array(elements))
    }
}
