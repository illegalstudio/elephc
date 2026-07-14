//! Purpose:
//! Parses class methods, properties, property hooks, and promoted property metadata.
//!
//! Called from:
//! - Class, trait, enum, and interface member parsing.
//!
//! Key details:
//! - Hook bodies, asymmetric visibility, types, defaults, and promotion remain aligned.

use super::*;

impl Parser {
    /// Parses one class/trait/enum method and returns constructor-promoted properties.
    pub(in crate::parser) fn parse_class_method_decl(
        &mut self,
        visibility: EvalVisibility,
        is_static: bool,
        is_abstract: bool,
        is_final: bool,
    ) -> Result<(EvalClassMethod, Vec<EvalClassProperty>), EvalParseError> {
        let source_start_line = self.current_line();
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
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
        } = self.parse_method_params(&name, true)?;
        if !promoted_properties.is_empty() && (is_abstract || is_static) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let return_type = self.parse_optional_return_type(EvalTypePosition::MethodReturn)?;
        let (body, source_end_line) = if is_abstract {
            let source_end_line = self.current_line();
            self.expect_semicolon()?;
            (Vec::new(), source_end_line)
        } else {
            let (body, source_end_line) = self.parse_block_with_end_line()?;
            let body = if promoted_assignments.is_empty() {
                body
            } else {
                promoted_assignments.into_iter().chain(body).collect()
            };
            (body, source_end_line)
        };
        Ok((
            EvalClassMethod::with_visibility_and_modifiers(
                name,
                visibility,
                is_static,
                is_abstract,
                is_final,
                params,
                body,
            )
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
            .with_parameter_types(parameter_types)
            .with_parameter_attributes(parameter_attributes)
            .with_parameter_defaults(parameter_defaults)
            .with_parameter_by_ref_flags(parameter_is_by_ref)
            .with_parameter_variadic_flags(parameter_is_variadic)
            .with_return_type(return_type),
            promoted_properties,
        ))
    }

    /// Parses one property declaration, including comma-separated simple properties.
    pub(in crate::parser) fn parse_class_property_decl(
        &mut self,
        visibility: EvalVisibility,
        set_visibility: Option<EvalVisibility>,
        is_static: bool,
        is_final: bool,
        is_readonly: bool,
        is_readonly_class: bool,
        is_abstract: bool,
    ) -> Result<(Vec<EvalClassProperty>, Vec<EvalClassMethod>), EvalParseError> {
        if is_static && is_readonly {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if set_visibility.is_some() && is_static {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if set_visibility.is_some_and(|set_visibility| {
            Self::eval_visibility_rank(set_visibility) > Self::eval_visibility_rank(visibility)
        }) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let effective_readonly = is_readonly || (is_readonly_class && !is_static);
        let property_type = self.parse_optional_property_type()?;
        if set_visibility.is_some() && property_type.is_none() {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let mut properties = Vec::new();
        let mut hook_methods = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let name = name.clone();
            self.advance();
            let default = if self.consume(TokenKind::Equal) {
                if is_abstract || effective_readonly {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                Some(self.parse_expr()?)
            } else {
                None
            };
            if is_abstract {
                if is_static || effective_readonly {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                let (requires_get_hook, requires_set_hook) =
                    self.parse_property_hook_contracts()?;
                let property = EvalClassProperty::with_visibility_static_final_and_readonly(
                    name,
                    visibility,
                    is_static,
                    is_final,
                    effective_readonly,
                    None,
                )
                .with_type(property_type)
                .with_set_visibility(set_visibility)
                .with_abstract_hook_contract(requires_get_hook, requires_set_hook);
                return Ok((vec![property], Vec::new()));
            }
            let default_is_some = default.is_some();
            if self.consume(TokenKind::Comma) {
                properties.push(
                    EvalClassProperty::with_visibility_static_final_and_readonly(
                        name,
                        visibility,
                        is_static,
                        is_final,
                        effective_readonly,
                        default,
                    )
                    .with_type(property_type.clone())
                    .with_set_visibility(set_visibility),
                );
                continue;
            }
            if !properties.is_empty() {
                self.expect_semicolon()?;
                properties.push(
                    EvalClassProperty::with_visibility_static_final_and_readonly(
                        name,
                        visibility,
                        is_static,
                        is_final,
                        effective_readonly,
                        default,
                    )
                    .with_type(property_type.clone())
                    .with_set_visibility(set_visibility),
                );
                break;
            }
            let (has_get_hook, has_set_hook, set_hook_type, parsed_hook_methods) = self
                .parse_property_hook_tail(
                    &name,
                    property_type.as_ref(),
                    is_static,
                    effective_readonly,
                    default_is_some,
                )?;
            if set_hook_type.is_some() && property_type.is_none() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let is_virtual = (has_get_hook || has_set_hook)
                && !property_hook_methods_use_backing_slot(&parsed_hook_methods, &name);
            properties.push(
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    name,
                    visibility,
                    is_static,
                    is_final,
                    effective_readonly,
                    default,
                )
                .with_type(property_type.clone())
                .with_set_hook_type(set_hook_type)
                .with_set_visibility(set_visibility)
                .with_hooks(has_get_hook, has_set_hook)
                .with_virtual(is_virtual),
            );
            hook_methods.extend(parsed_hook_methods);
            break;
        }
        Ok((properties, hook_methods))
    }

    /// Parses `;` or a concrete eval property hook block after one property declaration.
    pub(in crate::parser) fn parse_property_hook_tail(
        &mut self,
        property_name: &str,
        property_type: Option<&EvalParameterType>,
        is_static: bool,
        is_readonly: bool,
        has_default: bool,
    ) -> Result<(bool, bool, Option<EvalParameterType>, Vec<EvalClassMethod>), EvalParseError> {
        if self.consume(TokenKind::Semicolon) {
            return Ok((false, false, None, Vec::new()));
        }
        if !matches!(self.current(), TokenKind::LBrace) {
            return Err(EvalParseError::UnexpectedToken);
        }
        if is_static || is_readonly || has_default {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        self.advance();
        let mut has_get_hook = false;
        let mut has_set_hook = false;
        let mut set_hook_type = None;
        let mut methods = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            let (is_get, hook_set_type, method) =
                self.parse_property_hook_decl(property_name, property_type)?;
            if is_get {
                if has_get_hook {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                has_get_hook = true;
            } else {
                if has_set_hook {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                has_set_hook = true;
                set_hook_type = hook_set_type;
            }
            methods.push(method);
        }
        if !has_get_hook && !has_set_hook {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok((has_get_hook, has_set_hook, set_hook_type, methods))
    }

    /// Parses one concrete `get` or `set` property hook declaration.
    pub(in crate::parser) fn parse_property_hook_decl(
        &mut self,
        property_name: &str,
        property_type: Option<&EvalParameterType>,
    ) -> Result<(bool, Option<EvalParameterType>, EvalClassMethod), EvalParseError> {
        let returns_by_ref = self.consume(TokenKind::Ampersand);
        let TokenKind::Ident(hook_name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let is_get = ident_eq(hook_name, "get");
        let is_set = ident_eq(hook_name, "set");
        if !is_get && !is_set {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if returns_by_ref && !is_get {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let source_start_line = self.current_line();
        self.advance();
        let (params, set_hook_type) = if is_set {
            let (param, set_hook_type, has_explicit_param) =
                self.parse_property_set_hook_param()?;
            if has_explicit_param && set_hook_type.is_none() && property_type.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            (vec![param], set_hook_type)
        } else {
            (Vec::new(), None)
        };
        let (body, source_end_line) = match self.current() {
            TokenKind::Semicolon => return Err(EvalParseError::UnsupportedConstruct),
            TokenKind::FatArrow => {
                self.advance();
                let expr = self.parse_expr()?;
                let source_end_line = self.current_line();
                self.expect_semicolon()?;
                let body = if is_get {
                    vec![EvalStmt::Return(Some(expr))]
                } else {
                    vec![EvalStmt::PropertySet {
                        object: EvalExpr::LoadVar("this".to_string()),
                        property: property_name.to_string(),
                        value: expr,
                    }]
                };
                (body, source_end_line)
            }
            TokenKind::LBrace => self.parse_block_with_end_line()?,
            _ => return Err(EvalParseError::UnexpectedToken),
        };
        let method_name = if is_get {
            property_hook_get_method(property_name)
        } else {
            property_hook_set_method(property_name)
        };
        let mut method = EvalClassMethod::with_visibility_and_modifiers(
            method_name,
            EvalVisibility::Public,
            false,
            false,
            false,
            params,
            body,
        )
        .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line));
        if is_set {
            method = method.with_parameter_types(vec![
                set_hook_type.clone().or_else(|| property_type.cloned()),
            ]);
        }
        Ok((is_get, set_hook_type, method))
    }

    /// Parses an optional set-hook parameter list and returns the value variable metadata.
    pub(in crate::parser) fn parse_property_set_hook_param(
        &mut self,
    ) -> Result<(String, Option<EvalParameterType>, bool), EvalParseError> {
        if !self.consume(TokenKind::LParen) {
            return Ok(("value".to_string(), None, false));
        }
        let set_hook_type = self.parse_optional_property_type()?;
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::RParen)?;
        Ok((name, set_hook_type, true))
    }
}
