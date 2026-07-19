//! Purpose:
//! Parses callable parameters, promoted modifiers, and PHP type declarations.
//!
//! Called from:
//! - Function, method, closure, property, and hook parsing.
//!
//! Key details:
//! - Union/intersection atoms, nullability, defaults, variadics, and promotion stay indexed together.

use super::*;

impl Parser {
    /// Parses a method parameter list and records metadata plus promotion side effects.
    pub(in crate::parser) fn parse_method_params(
        &mut self,
        method_name: &str,
        allow_class_scope_types: bool,
    ) -> Result<ParsedMethodParams, EvalParseError> {
        let mut params = Vec::new();
        let mut parameter_attributes = Vec::new();
        let mut parameter_types = Vec::new();
        let mut parameter_defaults = Vec::new();
        let mut parameter_is_by_ref = Vec::new();
        let mut parameter_is_variadic = Vec::new();
        let mut promoted_properties = Vec::new();
        let mut promoted_assignments = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(ParsedMethodParams {
                params,
                parameter_attributes,
                parameter_types,
                parameter_defaults,
                parameter_is_by_ref,
                parameter_is_variadic,
                promoted_properties,
                promoted_assignments,
            });
        }
        loop {
            let attributes = self.parse_attribute_groups()?;
            let promotion = self.parse_promoted_parameter_modifiers()?;
            if promotion.is_some() && !method_name.eq_ignore_ascii_case("__construct") {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let param_type = if promotion.is_some() {
                self.parse_optional_promoted_property_type()?
            } else {
                let position = if allow_class_scope_types {
                    EvalTypePosition::MethodParameter
                } else {
                    EvalTypePosition::FunctionParameter
                };
                self.parse_optional_parameter_type(position)?
            };
            let is_by_ref = self.consume(TokenKind::Ampersand);
            let is_variadic = self.consume(TokenKind::Ellipsis);
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            if promotion.is_some() && is_variadic {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            if let Some((visibility, is_readonly)) = promotion {
                promoted_properties.push(
                    EvalClassProperty::with_visibility_static_final_and_readonly(
                        name.clone(),
                        visibility,
                        false,
                        false,
                        is_readonly,
                        None,
                    )
                    .with_type(param_type.clone())
                    .with_promoted()
                    .with_attributes(attributes.clone()),
                );
                promoted_assignments.push(promoted_property_assignment(name, is_by_ref));
            }
            params.push(name.clone());
            parameter_attributes.push(attributes);
            parameter_types.push(param_type);
            parameter_is_by_ref.push(is_by_ref);
            parameter_is_variadic.push(is_variadic);
            self.advance();
            let default = if self.consume(TokenKind::Equal) {
                if is_variadic {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                let default = self.parse_expr()?;
                if !method_parameter_default_is_supported(&default) {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                Some(default)
            } else {
                None
            };
            parameter_defaults.push(default);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if is_variadic {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            if matches!(self.current(), TokenKind::RParen) {
                return Err(EvalParseError::ExpectedVariable);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(ParsedMethodParams {
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            promoted_properties,
            promoted_assignments,
        })
    }

    /// Parses visibility and readonly modifiers on a promoted constructor parameter.
    pub(super) fn parse_promoted_parameter_modifiers(
        &mut self,
    ) -> Result<Option<(EvalVisibility, bool)>, EvalParseError> {
        let mut visibility = None;
        let mut is_readonly = false;
        let mut saw_modifier = false;
        loop {
            match self.current() {
                TokenKind::Ident(name) if ident_eq(name, "public") => {
                    if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    saw_modifier = true;
                    visibility = Some(EvalVisibility::Public);
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "protected") => {
                    if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    saw_modifier = true;
                    visibility = Some(EvalVisibility::Protected);
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "private") => {
                    if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    saw_modifier = true;
                    visibility = Some(EvalVisibility::Private);
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "readonly") => {
                    if is_readonly {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    saw_modifier = true;
                    is_readonly = true;
                    self.advance();
                }
                _ => break,
            }
        }
        if saw_modifier {
            Ok(Some((
                visibility.unwrap_or(EvalVisibility::Public),
                is_readonly,
            )))
        } else {
            Ok(None)
        }
    }

    /// Parses a constructor-promoted parameter type using PHP property-type restrictions.
    pub(super) fn parse_optional_promoted_property_type(
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

    /// Consumes a supported method parameter type and returns retained metadata.
    pub(super) fn parse_optional_parameter_type(
        &mut self,
        position: EvalTypePosition,
    ) -> Result<Option<EvalParameterType>, EvalParseError> {
        if matches!(
            self.current(),
            TokenKind::DollarIdent(_) | TokenKind::Ampersand | TokenKind::Ellipsis
        ) {
            return Ok(None);
        }
        self.parse_type_decl(position)
    }

    /// Consumes a supported function or method return type after `:`.
    pub(in crate::parser) fn parse_optional_return_type(
        &mut self,
        position: EvalTypePosition,
    ) -> Result<Option<EvalParameterType>, EvalParseError> {
        if !self.consume(TokenKind::Colon) {
            return Ok(None);
        }
        self.parse_type_decl(position)
    }

    /// Parses one PHP type declaration and returns retained eval metadata.
    pub(super) fn parse_type_decl(
        &mut self,
        position: EvalTypePosition,
    ) -> Result<Option<EvalParameterType>, EvalParseError> {
        let nullable_shorthand = self.consume(TokenKind::Question);
        if nullable_shorthand && matches!(self.current(), TokenKind::DollarIdent(_)) {
            return Err(EvalParseError::UnexpectedToken);
        }
        let first = self.parse_type_name(position)?;
        let mut variants = Vec::new();
        let mut allows_null = nullable_shorthand || matches!(first, None);
        if let Some(first) = first {
            variants.push(first);
        }
        if nullable_shorthand && matches!(self.current(), TokenKind::Pipe) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if nullable_shorthand
            && matches!(self.current(), TokenKind::Ampersand)
            && !self.next_token_starts_parameter_storage()
        {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if matches!(self.current(), TokenKind::Ampersand)
            && !self.next_token_starts_parameter_storage()
        {
            while self.consume(TokenKind::Ampersand) {
                let Some(variant) = self.parse_type_name(position)? else {
                    return Err(EvalParseError::UnsupportedConstruct);
                };
                variants.push(variant);
            }
            if type_variants_contain_standalone_return_only_atoms(&variants) {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            return Ok(Some(EvalParameterType::intersection(variants)));
        }
        while self.consume(TokenKind::Pipe) {
            match self.parse_type_name(position)? {
                Some(variant) => variants.push(variant),
                None => allows_null = true,
            }
        }
        if type_variants_contain_standalone_return_only_atoms(&variants)
            && (variants.len() != 1 || allows_null)
        {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok(Some(EvalParameterType::new(variants, allows_null)))
    }

    /// Returns whether `&` belongs to by-reference parameter storage.
    pub(super) fn next_token_starts_parameter_storage(&self) -> bool {
        matches!(self.peek(), TokenKind::DollarIdent(_) | TokenKind::Ellipsis)
    }

    /// Consumes one simple qualified method type name.
    pub(super) fn parse_type_name(
        &mut self,
        position: EvalTypePosition,
    ) -> Result<Option<EvalParameterTypeVariant>, EvalParseError> {
        match self.current() {
            TokenKind::Ident(_) | TokenKind::Backslash => {
                let name = self.parse_qualified_name()?;
                self.type_variant_from_name(name, position)
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Converts one parsed PHP type name to retained eval metadata.
    pub(super) fn type_variant_from_name(
        &self,
        name: ParsedQualifiedName,
        position: EvalTypePosition,
    ) -> Result<Option<EvalParameterTypeVariant>, EvalParseError> {
        if !name.absolute {
            let lower = name.name.to_ascii_lowercase();
            let builtin = match lower.as_str() {
                "array" => Some(EvalParameterTypeVariant::Array),
                "bool" => Some(EvalParameterTypeVariant::Bool),
                "callable" if matches!(position, EvalTypePosition::Property) => {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                "callable" => Some(EvalParameterTypeVariant::Callable),
                "float" => Some(EvalParameterTypeVariant::Float),
                "int" => Some(EvalParameterTypeVariant::Int),
                "iterable" => Some(EvalParameterTypeVariant::Iterable),
                "mixed" => Some(EvalParameterTypeVariant::Mixed),
                "never" if type_position_allows_return_only_atoms(position) => {
                    Some(EvalParameterTypeVariant::Never)
                }
                "null" => return Ok(None),
                "object" => Some(EvalParameterTypeVariant::Object),
                "string" => Some(EvalParameterTypeVariant::String),
                "void" if type_position_allows_return_only_atoms(position) => {
                    Some(EvalParameterTypeVariant::Void)
                }
                "void" | "never" => return Err(EvalParseError::UnsupportedConstruct),
                "static" if matches!(position, EvalTypePosition::MethodReturn) => {
                    Some(EvalParameterTypeVariant::Class(lower.to_string()))
                }
                "static" => return Err(EvalParseError::UnsupportedConstruct),
                "self" | "parent" if !type_position_allows_class_scope_atoms(position) => {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                "self" | "parent" => {
                    Some(EvalParameterTypeVariant::Class(lower.to_string()))
                }
                _ => None,
            };
            if let Some(builtin) = builtin {
                return Ok(Some(builtin));
            }
        }
        Ok(Some(EvalParameterTypeVariant::Class(
            self.resolve_class_name(name),
        )))
    }
}
