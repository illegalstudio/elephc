//! Purpose:
//! Parses class declarations, anonymous classes, modifiers, traits, constants, and member headers.
//!
//! Called from:
//! - Statement and expression parsing for class-like syntax.
//!
//! Key details:
//! - Class body collection preserves promoted-property and trait-adaptation side products.

use super::*;

impl Parser {
    /// Parses `[abstract|final|readonly] class Name [extends Parent] [implements Iface, ...] { ... }`.
    pub(in crate::parser) fn parse_class_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_class_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses a class declaration and attaches already parsed class attributes.
    pub(in crate::parser) fn parse_class_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let (is_abstract, is_final, is_readonly_class) = self.parse_class_decl_modifiers()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "class")) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let source_start_line = self.current_line();
        self.advance();
        let name = self.parse_class_like_decl_name()?;
        let parent = self.parse_class_parent_clause()?;
        let interfaces = self.parse_class_interface_clause()?;
        let body = self.parse_class_body_members(is_readonly_class)?;
        let source_location = EvalSourceLocation::new(source_start_line, body.source_end_line);
        self.consume_semicolon();
        Ok(vec![EvalStmt::ClassDecl(
            EvalClass::with_class_modifiers_traits_adaptations_and_constants(
                name,
                is_abstract,
                is_final,
                is_readonly_class,
                parent,
                interfaces,
                body.traits,
                body.trait_adaptations,
                body.constants,
                body.properties,
                body.methods,
            )
            .with_source_location(source_location)
            .with_attributes(attributes),
        )])
    }

    /// Parses `class [(args)] [extends Parent] [implements Iface, ...] { ... }` after `new`.
    pub(in crate::parser) fn parse_anonymous_class_expr(
        &mut self,
        is_readonly_class: bool,
    ) -> Result<EvalExpr, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "class")) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let source_start_line = self.current_line();
        self.advance();
        let args = if matches!(self.current(), TokenKind::LParen) {
            self.parse_call_args()?
        } else {
            Vec::new()
        };
        let parent = self.parse_class_parent_clause()?;
        let interfaces = self.parse_class_interface_clause()?;
        let body = self.parse_class_body_members(is_readonly_class)?;
        let source_location = EvalSourceLocation::new(source_start_line, body.source_end_line);
        let name = next_anonymous_class_name();
        let class = EvalClass::with_class_modifiers_traits_adaptations_and_constants(
            name,
            false,
            false,
            is_readonly_class,
            parent,
            interfaces,
            body.traits,
            body.trait_adaptations,
            body.constants,
            body.properties,
            body.methods,
        )
        .with_source_location(source_location)
        .with_anonymous();
        Ok(EvalExpr::NewAnonymousClass { class, args })
    }

    /// Parses and namespace-qualifies a declared class, interface, trait, or enum name.
    pub(super) fn parse_class_like_decl_name(&mut self) -> Result<String, EvalParseError> {
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        if is_reserved_class_like_name(name) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let qualified_name = self.qualify_name_in_current_namespace(name);
        self.advance();
        Ok(qualified_name)
    }

    /// Parses members inside a class body after relation clauses.
    pub(super) fn parse_class_body_members(
        &mut self,
        is_readonly_class: bool,
    ) -> Result<ParsedClassBody, EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut constants = Vec::new();
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        let mut traits = Vec::new();
        let mut trait_adaptations = Vec::new();
        loop {
            if matches!(self.current(), TokenKind::RBrace) {
                let source_end_line = self.current_line();
                self.advance();
                return Ok(ParsedClassBody {
                    source_end_line,
                    constants,
                    properties,
                    methods,
                    traits,
                    trait_adaptations,
                });
            }
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_class_member(
                is_readonly_class,
                &mut constants,
                &mut properties,
                &mut methods,
                &mut traits,
                &mut trait_adaptations,
            )?;
        }
    }

    /// Parses class-level `abstract`, `final`, and `readonly` modifiers before `class`.
    pub(in crate::parser) fn parse_class_decl_modifiers(
        &mut self,
    ) -> Result<(bool, bool, bool), EvalParseError> {
        let mut is_abstract = false;
        let mut is_final = false;
        let mut is_readonly_class = false;
        loop {
            match self.current() {
                TokenKind::Ident(name) if ident_eq(name, "abstract") => {
                    if is_abstract {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_abstract = true;
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "final") => {
                    if is_final {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_final = true;
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "readonly") => {
                    if is_readonly_class {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_readonly_class = true;
                    self.advance();
                }
                _ => return Ok((is_abstract, is_final, is_readonly_class)),
            }
        }
    }

    /// Parses an optional `extends Parent` class declaration clause.
    pub(in crate::parser) fn parse_class_parent_clause(&mut self) -> Result<Option<String>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "extends")) {
            return Ok(None);
        }
        self.advance();
        let parent = self.parse_class_reference_name(false)?;
        Ok(Some(self.resolve_class_name(parent)))
    }

    /// Parses an optional `implements Iface, ...` class declaration clause.
    pub(in crate::parser) fn parse_class_interface_clause(&mut self) -> Result<Vec<String>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "implements")) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut interfaces = Vec::new();
        loop {
            let interface = self.parse_class_reference_name(false)?;
            interfaces.push(self.resolve_class_name(interface));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        Ok(interfaces)
    }

    /// Parses one public property or method from an eval class body.
    pub(in crate::parser) fn parse_class_member(
        &mut self,
        is_readonly_class: bool,
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

        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "var")) {
            self.parse_legacy_var_property_member(
                attributes,
                visibility.is_some()
                    || is_static
                    || is_abstract
                    || is_final
                    || is_readonly
                    || set_visibility.is_some(),
                is_readonly_class,
                properties,
                methods,
            )?;
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

        let visibility = visibility.unwrap_or(EvalVisibility::Public);
        let (parsed_properties, mut hook_methods) = self.parse_class_property_decl(
            visibility,
            set_visibility,
            is_static,
            is_final,
            is_readonly,
            is_readonly_class,
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

    /// Parses PHP's legacy `var` public-property marker after the keyword is recognized.
    pub(super) fn parse_legacy_var_property_member(
        &mut self,
        attributes: Vec<EvalAttribute>,
        has_other_modifiers: bool,
        is_readonly_class: bool,
        properties: &mut Vec<EvalClassProperty>,
        methods: &mut Vec<EvalClassMethod>,
    ) -> Result<(), EvalParseError> {
        if has_other_modifiers {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        self.advance();
        let (parsed_properties, mut hook_methods) = self.parse_class_property_decl(
            EvalVisibility::Public,
            None,
            false,
            false,
            false,
            is_readonly_class,
            false,
        )?;
        properties.extend(
            parsed_properties
                .into_iter()
                .map(|property| property.with_attributes(attributes.clone())),
        );
        methods.append(&mut hook_methods);
        Ok(())
    }

    /// Parses optional attributes that decorate one class-like member.
    pub(in crate::parser) fn parse_optional_member_attributes(
        &mut self,
    ) -> Result<Vec<EvalAttribute>, EvalParseError> {
        if matches!(self.current(), TokenKind::AttributeStart) {
            self.parse_attribute_groups()
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses one eval class constant declaration, including comma-separated constants.
    pub(in crate::parser) fn parse_class_const_decl(
        &mut self,
        visibility: EvalVisibility,
        is_final: bool,
        attributes: &[EvalAttribute],
    ) -> Result<Vec<EvalClassConstant>, EvalParseError> {
        self.advance();
        let mut constants = Vec::new();
        loop {
            let TokenKind::Ident(name) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            if ident_eq(name, "class") {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let name = name.clone();
            self.advance();
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            constants.push(
                EvalClassConstant::with_visibility_and_final(name, visibility, is_final, value)
                    .with_attributes(attributes.to_vec()),
            );
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(constants)
    }

    /// Parses `use TraitName, OtherTrait;` or an adaptation block inside an eval class body.
    pub(in crate::parser) fn parse_class_trait_use(
        &mut self,
        traits: &mut Vec<String>,
        trait_adaptations: &mut Vec<EvalTraitAdaptation>,
    ) -> Result<(), EvalParseError> {
        self.advance();
        loop {
            let trait_name = self.parse_class_reference_name(false)?;
            traits.push(self.resolve_class_name(trait_name));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        if self.consume(TokenKind::LBrace) {
            while !self.consume(TokenKind::RBrace) {
                if matches!(self.current(), TokenKind::Eof) {
                    return Err(EvalParseError::UnexpectedEof);
                }
                trait_adaptations.push(self.parse_trait_adaptation()?);
                self.expect_semicolon()?;
            }
            self.consume_semicolon();
            Ok(())
        } else {
            self.expect_semicolon()
        }
    }

    /// Parses one `as` or `insteadof` trait adaptation clause.
    pub(in crate::parser) fn parse_trait_adaptation(&mut self) -> Result<EvalTraitAdaptation, EvalParseError> {
        let (trait_name, method) = self.parse_trait_adaptation_target()?;
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "as") => {
                self.advance();
                let visibility = self.parse_optional_trait_adaptation_visibility()?;
                let alias = if let TokenKind::Ident(alias) = self.current() {
                    let alias = alias.clone();
                    self.advance();
                    Some(alias)
                } else {
                    None
                };
                if visibility.is_none() && alias.is_none() {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                Ok(EvalTraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                })
            }
            TokenKind::Ident(name) if ident_eq(name, "insteadof") => {
                self.advance();
                let mut instead_of = Vec::new();
                loop {
                    let trait_name = self.parse_class_reference_name(false)?;
                    instead_of.push(self.resolve_class_name(trait_name));
                    if !self.consume(TokenKind::Comma) {
                        break;
                    }
                }
                if instead_of.is_empty() {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                Ok(EvalTraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                })
            }
            _ => Err(EvalParseError::UnsupportedConstruct),
        }
    }

    /// Parses the target before `as` or `insteadof`.
    pub(in crate::parser) fn parse_trait_adaptation_target(
        &mut self,
    ) -> Result<(Option<String>, String), EvalParseError> {
        let first = self.parse_qualified_name()?;
        if self.consume(TokenKind::DoubleColon) {
            if self.class_reference_name_is_reserved(&first, false) {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let TokenKind::Ident(method) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let method = method.clone();
            self.advance();
            Ok((Some(self.resolve_class_name(first)), method))
        } else {
            let method = first
                .name
                .rsplit('\\')
                .next()
                .filter(|segment| !segment.is_empty())
                .ok_or(EvalParseError::UnexpectedToken)?
                .to_string();
            Ok((None, method))
        }
    }

    /// Parses an optional visibility modifier inside a trait `as` adaptation.
    pub(in crate::parser) fn parse_optional_trait_adaptation_visibility(
        &mut self,
    ) -> Result<Option<EvalVisibility>, EvalParseError> {
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "public") => {
                self.advance();
                Ok(Some(EvalVisibility::Public))
            }
            TokenKind::Ident(name) if ident_eq(name, "protected") => {
                self.advance();
                Ok(Some(EvalVisibility::Protected))
            }
            TokenKind::Ident(name) if ident_eq(name, "private") => {
                self.advance();
                Ok(Some(EvalVisibility::Private))
            }
            _ => Ok(None),
        }
    }

    /// Parses method modifiers supported by eval class declarations.
    pub(in crate::parser) fn parse_class_member_modifiers(
        &mut self,
    ) -> Result<
        (
            Option<EvalVisibility>,
            Option<EvalVisibility>,
            bool,
            bool,
            bool,
            bool,
        ),
        EvalParseError,
    > {
        let mut visibility = None;
        let mut set_visibility = None;
        let mut is_static = false;
        let mut is_abstract = false;
        let mut is_final = false;
        let mut is_readonly = false;
        loop {
            match self.current() {
                TokenKind::Ident(name) if ident_eq(name, "public") => {
                    self.advance();
                    if self.consume_set_marker()? {
                        if set_visibility.is_some() {
                            return Err(EvalParseError::UnsupportedConstruct);
                        }
                        set_visibility = Some(EvalVisibility::Public);
                    } else if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    } else {
                        visibility = Some(EvalVisibility::Public);
                    }
                }
                TokenKind::Ident(name) if ident_eq(name, "protected") => {
                    self.advance();
                    if self.consume_set_marker()? {
                        if set_visibility.is_some() {
                            return Err(EvalParseError::UnsupportedConstruct);
                        }
                        set_visibility = Some(EvalVisibility::Protected);
                    } else if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    } else {
                        visibility = Some(EvalVisibility::Protected);
                    }
                }
                TokenKind::Ident(name) if ident_eq(name, "private") => {
                    self.advance();
                    if self.consume_set_marker()? {
                        if set_visibility.is_some() {
                            return Err(EvalParseError::UnsupportedConstruct);
                        }
                        set_visibility = Some(EvalVisibility::Private);
                    } else if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    } else {
                        visibility = Some(EvalVisibility::Private);
                    }
                }
                TokenKind::Ident(name) if ident_eq(name, "static") => {
                    if is_static {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_static = true;
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "abstract") => {
                    if is_abstract {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_abstract = true;
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "final") => {
                    if is_final {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_final = true;
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "readonly") => {
                    if is_readonly {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    is_readonly = true;
                    self.advance();
                }
                TokenKind::Ident(name) if is_unsupported_class_member_modifier(name) => {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                _ => {
                    return Ok((
                        visibility,
                        set_visibility,
                        is_static,
                        is_abstract,
                        is_final,
                        is_readonly,
                    ))
                }
            }
        }
    }

    /// Consumes a PHP asymmetric visibility `(set)` marker after a visibility keyword.
    pub(super) fn consume_set_marker(&mut self) -> Result<bool, EvalParseError> {
        if !self.consume(TokenKind::LParen) {
            return Ok(false);
        }
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "set") => self.advance(),
            _ => return Err(EvalParseError::UnsupportedConstruct),
        }
        self.expect(TokenKind::RParen)?;
        Ok(true)
    }

    /// Returns a comparable visibility rank where larger means less restrictive.
    pub(super) fn eval_visibility_rank(visibility: EvalVisibility) -> u8 {
        match visibility {
            EvalVisibility::Private => 1,
            EvalVisibility::Protected => 2,
            EvalVisibility::Public => 3,
        }
    }
}
