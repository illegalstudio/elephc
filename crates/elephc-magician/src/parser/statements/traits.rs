//! Purpose:
//! Parses trait declarations and trait-specific members.
//!
//! Called from:
//! - Statement dispatch for attributed and plain trait declarations.
//!
//! Key details:
//! - Trait properties and hooks reuse class-member parsing while preserving metadata.

use super::*;

impl Parser {
    /// Parses `trait Name { ... }` declarations into dynamic trait metadata.
    pub(in crate::parser) fn parse_trait_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_trait_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses a trait declaration and attaches already parsed class-like attributes.
    pub(in crate::parser) fn parse_trait_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
        self.advance();
        let name = self.parse_class_like_decl_name()?;
        self.expect(TokenKind::LBrace)?;
        let mut traits = Vec::new();
        let mut trait_adaptations = Vec::new();
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
            self.parse_trait_member(
                &mut constants,
                &mut properties,
                &mut methods,
                &mut traits,
                &mut trait_adaptations,
            )?;
        };
        self.consume_semicolon();
        Ok(vec![EvalStmt::TraitDecl(
            EvalTrait::with_constants_traits_adaptations(
                name,
                constants,
                properties,
                methods,
                traits,
                trait_adaptations,
            )
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
            .with_attributes(attributes),
        )])
    }

    /// Parses one property or method from an eval trait body.
    pub(in crate::parser) fn parse_trait_member(
        &mut self,
        constants: &mut Vec<EvalClassConstant>,
        properties: &mut Vec<EvalClassProperty>,
        methods: &mut Vec<EvalClassMethod>,
        traits: &mut Vec<String>,
        trait_adaptations: &mut Vec<EvalTraitAdaptation>,
    ) -> Result<(), EvalParseError> {
        let attributes = self.parse_optional_member_attributes()?;
        let (visibility, set_visibility, is_static, is_abstract, is_final, is_readonly) =
            self.parse_class_member_modifiers()?;
        if is_abstract && is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if visibility.is_none()
            && !is_static
            && !is_abstract
            && !is_final
            && !is_readonly
            && set_visibility.is_none()
            && matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "use"))
        {
            if !attributes.is_empty() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            self.parse_class_trait_use(traits, trait_adaptations)?;
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || is_abstract || is_readonly || set_visibility.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants.extend(
                self.parse_class_const_decl(
                    visibility.unwrap_or(EvalVisibility::Public),
                    is_final,
                    &attributes,
                )?,
            );
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            if is_readonly || set_visibility.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let (method, promoted_properties) = self.parse_class_method_decl(
                visibility.unwrap_or(EvalVisibility::Public),
                is_static,
                is_abstract,
                is_final,
            )?;
            properties.extend(promoted_properties);
            methods.push(method.with_attributes(attributes));
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "var")) {
            self.parse_legacy_var_property_member(
                attributes,
                visibility.is_some()
                    || is_static
                    || is_abstract
                    || is_final
                    || is_readonly
                    || set_visibility.is_some(),
                false,
                properties,
                methods,
            )?;
            return Ok(());
        }
        let visibility = visibility.unwrap_or(EvalVisibility::Public);
        let (parsed_properties, mut hook_methods) = self.parse_class_property_decl(
            visibility,
            set_visibility,
            is_static,
            is_final,
            is_readonly,
            false,
            is_abstract,
        )?;
        properties.extend(
            parsed_properties
                .into_iter()
                .map(|property| property.with_attributes(attributes.clone())),
        );
        methods.append(&mut hook_methods);
        Ok(())
    }
}
