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
use crate::eval_ir::{EvalAttribute, EvalStmt};
use std::collections::HashSet;

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

    /// Retains source spelling while resolving first-class function names and fallback metadata.
    pub(super) fn function_callable_expr(&self, name: String) -> EvalExpr {
        let (base, suffix) = name.find('<').map_or((name.as_str(), ""), |index| (&name[..index], &name[index..]));
        if let Some(imported) = self.imports.resolve_function(base) {
            return Self::function_callable_value(format!("{imported}{suffix}"), None);
        }
        if self.namespace.is_empty() {
            Self::function_callable_value(name, None)
        } else {
            Self::function_callable_value(
                self.qualify_name_in_current_namespace(&name),
                Some(name),
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
        self.parse_closure_expr_with_attributes(is_static, Vec::new())
    }

    /// Parses an attributed anonymous function expression into a runtime eval closure payload.
    pub(super) fn parse_closure_expr_with_attributes(
        &mut self,
        is_static: bool,
        attributes: Vec<EvalAttribute>,
    ) -> Result<EvalExpr, EvalParseError> {
        let source_start_line = self.current_line();
        if is_static {
            self.advance();
        }
        self.advance();
        // PHP's return-by-reference marker sits between the keyword and the parameter list. It
        // was never consumed here, so `function &()` and `fn &()` died on the `&`.
        let returns_by_ref = self.consume(TokenKind::Ampersand);
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
            .with_attributes(attributes)
            .with_parameter_attributes(parameter_attributes)
            .with_parameter_types(parameter_types)
            .with_parameter_defaults(parameter_defaults)
            .with_parameter_by_ref_flags(parameter_is_by_ref)
            .with_parameter_variadic_flags(parameter_is_variadic)
            .with_return_type(return_type)
            .with_returns_by_ref(returns_by_ref);
        Ok(EvalExpr::Closure {
            function,
            captures,
            is_static,
        })
    }

    /// Parses attributes followed by an anonymous `function` or arrow `fn` expression.
    pub(super) fn parse_attributed_closure_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        let attributes = self.parse_attribute_groups()?;
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "function") => {
                self.parse_closure_expr_with_attributes(false, attributes)
            }
            TokenKind::Ident(name) if ident_eq(name, "fn") => {
                self.parse_arrow_closure_expr(false, attributes)
            }
            TokenKind::Ident(name)
                if ident_eq(name, "static")
                    && matches!(self.peek(), TokenKind::Ident(next) if ident_eq(next, "function")) =>
            {
                self.parse_closure_expr_with_attributes(true, attributes)
            }
            TokenKind::Ident(name)
                if ident_eq(name, "static")
                    && matches!(self.peek(), TokenKind::Ident(next) if ident_eq(next, "fn")) =>
            {
                self.parse_arrow_closure_expr(true, attributes)
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Parses a PHP arrow function and infers its implicit by-value captures.
    pub(super) fn parse_arrow_closure_expr(
        &mut self,
        is_static: bool,
        attributes: Vec<EvalAttribute>,
    ) -> Result<EvalExpr, EvalParseError> {
        let source_start_line = self.current_line();
        if is_static {
            self.advance();
        }
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "fn")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        // PHP's return-by-reference marker sits between the keyword and the parameter list. It
        // was never consumed here, so `function &()` and `fn &()` died on the `&`.
        let returns_by_ref = self.consume(TokenKind::Ampersand);
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
        let return_type = self.parse_optional_return_type(EvalTypePosition::FunctionReturn)?;
        self.expect(TokenKind::FatArrow)?;
        let body = self.parse_expr()?;
        let source_end_line = self.current_line();
        let captures = infer_arrow_closure_captures(&body, &params);
        let function = EvalFunction::new(
            next_closure_function_name(),
            params,
            vec![EvalStmt::Return(Some(body))],
        )
        .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
        .with_attributes(attributes)
        .with_parameter_attributes(parameter_attributes)
        .with_parameter_types(parameter_types)
        .with_parameter_defaults(parameter_defaults)
        .with_parameter_by_ref_flags(parameter_is_by_ref)
        .with_parameter_variadic_flags(parameter_is_variadic)
        .with_return_type(return_type)
        .with_returns_by_ref(returns_by_ref);
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
            // PHP 8.0 accepts a trailing comma after the last capture: `use ($a, $b,)`.
            if matches!(self.current(), TokenKind::RParen) {
                break;
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
            if self.consume(TokenKind::Ellipsis) {
                // PHP's array unpacking. It reuses the variadic token but is a THIRD element
                // shape: the operand's integer keys are renumbered and its string keys kept, so
                // it is neither `Value` nor `KeyValue`.
                let value = self.parse_expr()?;
                elements.push(EvalArrayElement::Spread(value));
                if !self.consume(TokenKind::Comma) {
                    break;
                }
                if self.consume(close.clone()) {
                    return Ok(EvalExpr::Array(elements));
                }
                continue;
            }
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
            // `[...$rest]`. PHP 8.1 unpacks an array or Traversable here, renumbering integer keys
            // and keeping string ones; the operand is an ordinary expression and carries no key of
            // its own, so it never reaches the `=>` branch below.
            if self.consume(TokenKind::Ellipsis) {
                let value = self.parse_expr()?;
                elements.push(EvalArrayElement::Spread(value));
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

/// Collects variables read by an arrow body, excluding parameters and PHP superglobals.
fn infer_arrow_closure_captures(
    body: &EvalExpr,
    params: &[String],
) -> Vec<EvalClosureCapture> {
    let mut names = Vec::new();
    collect_arrow_expr_variables(body, &mut names);
    let params = params.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|name| {
            !params.contains(name.as_str())
                && name != "this"
                && !is_arrow_superglobal(name)
                && seen.insert(name.clone())
        })
        .map(|name| EvalClosureCapture::new(name, false))
        .collect()
}

/// Recursively records variable reads that occur while evaluating one arrow expression.
fn collect_arrow_expr_variables(expr: &EvalExpr, names: &mut Vec<String>) {
    match expr {
        EvalExpr::LoadVar(name) => names.push(name.clone()),
        EvalExpr::Array(elements) => {
            for element in elements {
                match element {
                    EvalArrayElement::Value(value)
                    | EvalArrayElement::Reference(value)
                    | EvalArrayElement::Spread(value) => {
                        collect_arrow_expr_variables(value, names);
                    }
                    EvalArrayElement::KeyValue { key, value }
                    | EvalArrayElement::KeyReference { key, value } => {
                        collect_arrow_expr_variables(key, names);
                        collect_arrow_expr_variables(value, names);
                    }
                }
            }
        }
        EvalExpr::ArrayDestructureAssign { value, .. }
        | EvalExpr::ArrayAppendSlot { target: value } => {
            collect_arrow_expr_variables(value, names);
        }
        // The bound name is written, not read, so only the source is a capture.
        EvalExpr::ReferenceBindAssign { source, .. } => {
            collect_arrow_expr_variables(source, names);
        }
        EvalExpr::ArrayGet { array, index } => {
            collect_arrow_expr_variables(array, names);
            collect_arrow_expr_variables(index, names);
        }
        EvalExpr::Call { args, .. }
        | EvalExpr::NamespacedCall { args, .. }
        | EvalExpr::NewObject { args, .. }
        | EvalExpr::StaticMethodCall { args, .. } => {
            collect_arrow_call_args(args, names);
        }
        EvalExpr::Closure { captures, .. } => {
            names.extend(captures.iter().map(|capture| capture.name().to_string()));
        }
        EvalExpr::Cast { expr, .. }
        | EvalExpr::InvokableCallable { object: expr }
        | EvalExpr::DynamicClassNameFetch { class_name: expr }
        | EvalExpr::Include { path: expr, .. }
        | EvalExpr::Clone(expr)
        | EvalExpr::DynamicVar(expr)
        | EvalExpr::Print(expr)
        | EvalExpr::Throw(expr)
        | EvalExpr::Unary { expr, .. } => collect_arrow_expr_variables(expr, names),
        EvalExpr::MethodCallable { object, method }
        | EvalExpr::DynamicPropertyGet { object, property: method }
        | EvalExpr::DynamicClassConstantNameFetch {
            class_name: object,
            constant: method,
        }
        | EvalExpr::DynamicStaticPropertyNameGet {
            class_name: object,
            property: method,
        }
        | EvalExpr::NullsafeDynamicPropertyGet {
            object,
            property: method,
        }
        | EvalExpr::Binary {
            left: object,
            right: method,
            ..
        }
        | EvalExpr::NullCoalesce {
            value: object,
            default: method,
        }
        | EvalExpr::NullCoalesceAssign {
            target: object,
            default: method,
        }
        | EvalExpr::CompoundAssign {
            target: object,
            value: method,
            ..
        }
        | EvalExpr::ArrayAppendAssign {
            target: object,
            value: method,
        }
        | EvalExpr::ReferenceBind {
            target: object,
            source: method,
        }
        | EvalExpr::Assign {
            target: object,
            value: method,
        } => {
            collect_arrow_expr_variables(object, names);
            collect_arrow_expr_variables(method, names);
        }
        EvalExpr::PostfixIncDec { target, .. } => {
            collect_arrow_expr_variables(target, names);
        }
        EvalExpr::StaticMethodCallable { method, .. }
        | EvalExpr::DynamicStaticPropertyGet {
            class_name: method, ..
        }
        | EvalExpr::DynamicClassConstantFetch {
            class_name: method, ..
        }
        | EvalExpr::NullsafePropertyGet { object: method, .. }
        | EvalExpr::PropertyGet { object: method, .. } => {
            collect_arrow_expr_variables(method, names);
        }
        EvalExpr::DynamicStaticMethodCallable { class_name, method } => {
            collect_arrow_expr_variables(class_name, names);
            collect_arrow_expr_variables(method, names);
        }
        EvalExpr::DynamicCall { callee, args }
        | EvalExpr::DynamicNewObject {
            class_name: callee,
            args,
        } => {
            collect_arrow_expr_variables(callee, names);
            collect_arrow_call_args(args, names);
        }
        EvalExpr::DynamicMethodCall {
            object,
            method,
            args,
        }
        | EvalExpr::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        }
        | EvalExpr::DynamicStaticMethodCall {
            class_name: object,
            method,
            args,
        } => {
            collect_arrow_expr_variables(object, names);
            collect_arrow_expr_variables(method, names);
            collect_arrow_call_args(args, names);
        }
        EvalExpr::InstanceOf { value, target } => {
            collect_arrow_expr_variables(value, names);
            if let EvalInstanceOfTarget::Expr(target) = target {
                collect_arrow_expr_variables(target, names);
            }
        }
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => {
            collect_arrow_expr_variables(subject, names);
            for arm in arms {
                for pattern in &arm.patterns {
                    collect_arrow_expr_variables(pattern, names);
                }
                collect_arrow_expr_variables(&arm.value, names);
            }
            if let Some(default) = default {
                collect_arrow_expr_variables(default, names);
            }
        }
        EvalExpr::MethodCall { object, args, .. }
        | EvalExpr::NullsafeMethodCall { object, args, .. } => {
            collect_arrow_expr_variables(object, names);
            collect_arrow_call_args(args, names);
        }
        EvalExpr::NewAnonymousClass { args, .. } => collect_arrow_call_args(args, names),
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_arrow_expr_variables(condition, names);
            if let Some(then_branch) = then_branch {
                collect_arrow_expr_variables(then_branch, names);
            }
            collect_arrow_expr_variables(else_branch, names);
        }
        EvalExpr::Const(_)
        | EvalExpr::ConstFetch(_)
        | EvalExpr::FunctionCallable { .. }
        | EvalExpr::NamespacedConstFetch { .. }
        | EvalExpr::Magic(_)
        | EvalExpr::StaticPropertyGet { .. }
        | EvalExpr::ClassConstantFetch { .. }
        | EvalExpr::ClassNameFetch { .. }
        | EvalExpr::DeferredDefaultError(_) => {}
    }
}

/// Records every expression evaluated by source-order call arguments.
fn collect_arrow_call_args(args: &[EvalCallArg], names: &mut Vec<String>) {
    for arg in args {
        collect_arrow_expr_variables(arg.value(), names);
    }
}

/// Returns whether a variable is globally available without lexical capture.
fn is_arrow_superglobal(name: &str) -> bool {
    matches!(
        name,
        "GLOBALS"
            | "_SERVER"
            | "_GET"
            | "_POST"
            | "_FILES"
            | "_COOKIE"
            | "_SESSION"
            | "_REQUEST"
            | "_ENV"
    )
}

