//! Purpose:
//! Parses interfaces, parents, abstract methods, properties, and hook contracts.
//!
//! Called from:
//! - Statement dispatch for attributed and plain interface declarations.
//!
//! Key details:
//! - Interface property hook requirements and types remain declarative EvalIR metadata.

use super::*;

impl Parser {
    /// Parses `interface Name [extends Parent, ...] { function name(...); }`.
    pub(in crate::parser) fn parse_interface_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_interface_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses an interface declaration and attaches already parsed class-like attributes.
    pub(in crate::parser) fn parse_interface_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
        self.advance();
        let name = self.parse_class_like_decl_name()?;
        let parents = self.parse_interface_parent_clause()?;
        self.expect(TokenKind::LBrace)?;
        let mut constants = Vec::new();
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        let source_end_line = loop {
            if matches!(self.current(), TokenKind::RBrace) {
                let source_end_line = self.current_line();
                self.advance();
                break source_end_line;
            }
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_interface_member(&mut constants, &mut properties, &mut methods)?;
        };
        self.consume_semicolon();
        Ok(vec![EvalStmt::InterfaceDecl(
            EvalInterface::with_constants_and_properties(
                name, parents, constants, properties, methods,
            )
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
            .with_attributes(attributes),
        )])
    }

    /// Parses an optional `extends Parent, ...` interface declaration clause.
    pub(in crate::parser) fn parse_interface_parent_clause(&mut self) -> Result<Vec<String>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "extends")) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut parents = Vec::new();
        loop {
            let parent = self.parse_class_reference_name(false)?;
            parents.push(self.resolve_class_name(parent));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        Ok(parents)
    }

    /// Parses one eval interface constant, property contract, or method signature.
    pub(in crate::parser) fn parse_interface_member(
        &mut self,
        constants: &mut Vec<EvalClassConstant>,
        properties: &mut Vec<EvalInterfaceProperty>,
        methods: &mut Vec<EvalInterfaceMethod>,
    ) -> Result<(), EvalParseError> {
        let attributes = self.parse_optional_member_attributes()?;
        let mut is_static = false;
        let mut is_final = false;
        let mut saw_public = false;
        let mut set_visibility = None;
        loop {
            match self.current() {
                TokenKind::Ident(name) if ident_eq(name, "public") => {
                    self.advance();
                    if self.consume_set_marker()? {
                        if set_visibility.is_some() {
                            return Err(EvalParseError::UnsupportedConstruct);
                        }
                        set_visibility = Some(EvalVisibility::Public);
                    } else if saw_public {
                        return Err(EvalParseError::UnsupportedConstruct);
                    } else {
                        saw_public = true;
                    }
                }
                TokenKind::Ident(name) if ident_eq(name, "protected") => {
                    self.advance();
                    if !self.consume_set_marker()? || set_visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    set_visibility = Some(EvalVisibility::Protected);
                }
                TokenKind::Ident(name) if ident_eq(name, "private") => {
                    self.advance();
                    if !self.consume_set_marker()? || set_visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    set_visibility = Some(EvalVisibility::Private);
                }
                TokenKind::Ident(name) if ident_eq(name, "static") => {
                    if is_static {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_static = true;
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "final") => {
                    if is_final {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_final = true;
                    self.advance();
                }
                TokenKind::Ident(name) if is_unsupported_class_member_modifier(name) => {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                _ => break,
            }
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || set_visibility.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants.extend(self.parse_class_const_decl(
                EvalVisibility::Public,
                is_final,
                &attributes,
            )?);
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            if is_final || set_visibility.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            methods.push(
                self.parse_interface_method_decl_after_function_keyword(is_static)?
                    .with_attributes(attributes),
            );
            return Ok(());
        }
        if is_static || is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        properties.push(
            self.parse_interface_property_decl(set_visibility)?
                .with_attributes(attributes),
        );
        Ok(())
    }

    /// Parses one eval interface method signature after `function` has been selected.
    pub(in crate::parser) fn parse_interface_method_decl_after_function_keyword(
        &mut self,
        is_static: bool,
    ) -> Result<EvalInterfaceMethod, EvalParseError> {
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
        if !promoted_properties.is_empty() || !promoted_assignments.is_empty() {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let return_type = self.parse_optional_return_type(EvalTypePosition::MethodReturn)?;
        let source_end_line = self.current_line();
        self.expect_semicolon()?;
        Ok(EvalInterfaceMethod::new(name, params)
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
            .with_static(is_static)
            .with_parameter_types(parameter_types)
            .with_parameter_attributes(parameter_attributes)
            .with_parameter_defaults(parameter_defaults)
            .with_parameter_by_ref_flags(parameter_is_by_ref)
            .with_parameter_variadic_flags(parameter_is_variadic)
            .with_return_type(return_type))
    }

    /// Parses one interface property hook contract.
    pub(in crate::parser) fn parse_interface_property_decl(
        &mut self,
        set_visibility: Option<EvalVisibility>,
    ) -> Result<EvalInterfaceProperty, EvalParseError> {
        let property_type = self.parse_optional_property_type()?;
        if set_visibility.is_some() && property_type.is_none() {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        if matches!(self.current(), TokenKind::Equal) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let (requires_get, requires_set) = self.parse_interface_property_hook_contracts()?;
        if set_visibility.is_some() && !requires_set {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok(EvalInterfaceProperty::new(name, requires_get, requires_set)
            .with_type(property_type)
            .with_set_visibility(set_visibility))
    }

    /// Parses `{ get; set; }` hook contracts for an abstract or interface property.
    pub(in crate::parser) fn parse_property_hook_contracts(&mut self) -> Result<(bool, bool), EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut requires_get = false;
        let mut requires_set = false;
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
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
            self.advance();
            if matches!(
                self.current(),
                TokenKind::LParen | TokenKind::FatArrow | TokenKind::LBrace
            ) {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            self.expect_semicolon()?;
            if is_get {
                if requires_get {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                requires_get = true;
            } else {
                if requires_set {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                requires_set = true;
            }
        }
        if !requires_get && !requires_set {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok((requires_get, requires_set))
    }

    /// Parses `{ get; set; }` hook contracts for an interface property.
    pub(in crate::parser) fn parse_interface_property_hook_contracts(
        &mut self,
    ) -> Result<(bool, bool), EvalParseError> {
        self.parse_property_hook_contracts()
    }

    /// Parses retained property type metadata before the `$property` token.
    pub(in crate::parser) fn parse_optional_property_type(
        &mut self,
    ) -> Result<Option<EvalParameterType>, EvalParseError> {
        if matches!(
            self.current(),
            TokenKind::DollarIdent(_) | TokenKind::Ampersand | TokenKind::Ellipsis
        ) {
            return Ok(None);
        }
        self.parse_type_decl(EvalTypePosition::Property)
    }
}
