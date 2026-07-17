//! Purpose:
//! Parses enum declarations, backing types, cases, traits, and methods.
//!
//! Called from:
//! - Statement dispatch for attributed and plain enum declarations.
//!
//! Key details:
//! - Backed and unit cases retain source metadata and literal backing expressions.

use super::*;

impl Parser {
    /// Parses `enum Name [: int|string] [implements Iface, ...] { ... }`.
    pub(in crate::parser) fn parse_enum_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_enum_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses an enum declaration and attaches already parsed class-like attributes.
    pub(in crate::parser) fn parse_enum_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
        self.advance();
        let name = self.parse_class_like_decl_name()?;
        let backing_type = self.parse_enum_backing_type()?;
        let interfaces = self.parse_class_interface_clause()?;
        self.expect(TokenKind::LBrace)?;
        let mut traits = Vec::new();
        let mut trait_adaptations = Vec::new();
        let mut cases = Vec::new();
        let mut constants = Vec::new();
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
            self.parse_enum_member(
                &mut cases,
                &mut constants,
                &mut methods,
                &mut traits,
                &mut trait_adaptations,
            )?;
        };
        self.consume_semicolon();
        Ok(vec![EvalStmt::EnumDecl(
            EvalEnum::with_members_traits_adaptations(
                name,
                backing_type,
                interfaces,
                cases,
                constants,
                methods,
                traits,
                trait_adaptations,
            )
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
            .with_attributes(attributes),
        )])
    }

    /// Parses an optional backed-enum scalar type after the enum name.
    pub(in crate::parser) fn parse_enum_backing_type(
        &mut self,
    ) -> Result<Option<EvalEnumBackingType>, EvalParseError> {
        if !self.consume(TokenKind::Colon) {
            return Ok(None);
        }
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let backing_type = if ident_eq(name, "int") {
            EvalEnumBackingType::Int
        } else if ident_eq(name, "string") {
            EvalEnumBackingType::String
        } else {
            return Err(EvalParseError::UnsupportedConstruct);
        };
        self.advance();
        Ok(Some(backing_type))
    }

    /// Parses one enum case, constant, or method declaration.
    pub(in crate::parser) fn parse_enum_member(
        &mut self,
        cases: &mut Vec<EvalEnumCase>,
        constants: &mut Vec<EvalClassConstant>,
        methods: &mut Vec<EvalClassMethod>,
        traits: &mut Vec<String>,
        trait_adaptations: &mut Vec<EvalTraitAdaptation>,
    ) -> Result<(), EvalParseError> {
        let attributes = self.parse_optional_member_attributes()?;
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "case")) {
            cases.push(self.parse_enum_case_decl()?.with_attributes(attributes));
            return Ok(());
        }
        let (visibility, set_visibility, is_static, is_abstract, is_final, is_readonly) =
            self.parse_class_member_modifiers()?;
        if is_abstract || is_readonly || set_visibility.is_some() {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if visibility.is_none()
            && !is_static
            && !is_final
            && matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "use"))
        {
            if !attributes.is_empty() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            self.parse_class_trait_use(traits, trait_adaptations)?;
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static {
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
            let (method, promoted_properties) = self.parse_class_method_decl(
                visibility.unwrap_or(EvalVisibility::Public),
                is_static,
                false,
                is_final,
            )?;
            if !promoted_properties.is_empty() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            methods.push(method.with_attributes(attributes));
            return Ok(());
        }
        Err(EvalParseError::UnsupportedConstruct)
    }

    /// Parses `case Name;` or `case Name = expr;` inside an eval enum body.
    pub(in crate::parser) fn parse_enum_case_decl(&mut self) -> Result<EvalEnumCase, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        let value = if self.consume(TokenKind::Equal) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_semicolon()?;
        Ok(EvalEnumCase::new(name, value))
    }
}
