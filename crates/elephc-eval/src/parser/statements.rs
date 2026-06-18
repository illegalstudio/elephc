//! Purpose:
//! Parses PHP eval statements, declarations, control structures, and statement blocks into EvalIR.
//!
//! Called from:
//! - `crate::parser::state::Parser::parse_program()`.
//!
//! Key details:
//! - Statement parsing expands multi-variable constructs such as `unset($a, $b)` into multiple EvalIR statements.
//! - Namespace/use parsing lives here because declarations are statement-level syntax in PHP.

use super::cursor::*;
use super::state::*;
use crate::errors::EvalParseError;
use crate::eval_ir::{
    EvalCatch, EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalEnum,
    EvalEnumBackingType, EvalEnumCase, EvalExpr, EvalInterface, EvalInterfaceMethod, EvalStmt,
    EvalSwitchCase, EvalTrait, EvalTraitAdaptation, EvalVisibility,
};
use crate::lexer::TokenKind;

impl Parser {
    /// Parses one source statement, expanding `unset($a, $b)` to one statement per variable.
    pub(super) fn parse_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "break") => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Break])
            }
            TokenKind::Ident(name) if ident_eq(name, "continue") => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Continue])
            }
            TokenKind::Ident(name) if ident_eq(name, "do") => self.parse_do_while_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "echo") => {
                self.advance();
                let mut statements = vec![EvalStmt::Echo(self.parse_expr()?)];
                while self.consume(TokenKind::Comma) {
                    statements.push(EvalStmt::Echo(self.parse_expr()?));
                }
                self.expect_semicolon()?;
                Ok(statements)
            }
            TokenKind::Ident(name) if ident_eq(name, "for") => self.parse_for_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "foreach") => self.parse_foreach_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "abstract") || ident_eq(name, "final") => {
                self.parse_class_decl_stmt()
            }
            TokenKind::Ident(name) if ident_eq(name, "class") => self.parse_class_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "enum") => self.parse_enum_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "function") => self.parse_function_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "global") => self.parse_global_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "if") => self.parse_if_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "interface") => {
                self.parse_interface_decl_stmt()
            }
            TokenKind::Ident(name) if ident_eq(name, "namespace") => self.parse_namespace_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "return") => {
                self.advance();
                if self.consume_semicolon() {
                    return Ok(vec![EvalStmt::Return(None)]);
                }
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Return(Some(expr))])
            }
            TokenKind::Ident(name)
                if ident_eq(name, "static") && self.current_starts_static_property_assignment() =>
            {
                self.parse_static_property_set_stmt(true)
            }
            TokenKind::Ident(name)
                if ident_eq(name, "static") && !matches!(self.peek(), TokenKind::DoubleColon) =>
            {
                self.parse_static_var_stmt()
            }
            TokenKind::Ident(name) if ident_eq(name, "switch") => self.parse_switch_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "throw") => self.parse_throw_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "try") => self.parse_try_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "trait") => self.parse_trait_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "unset") => self.parse_unset_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "use") && self.allow_use_imports => {
                self.parse_use_stmt()
            }
            TokenKind::Ident(name) if ident_eq(name, "use") => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::Ident(name) if ident_eq(name, "while") => self.parse_while_stmt(),
            TokenKind::Ident(name) if is_unsupported_statement_keyword(name) => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::Ident(_) | TokenKind::Backslash
                if self.current_starts_static_property_assignment() =>
            {
                self.parse_static_property_set_stmt(true)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus => self.parse_prefix_inc_dec_stmt(true),
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(true)
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_stmt(name.clone())
            }
            TokenKind::DollarIdent(name)
                if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) =>
            {
                self.parse_postfix_inc_dec_stmt(name.clone(), true)
            }
            TokenKind::DollarIdent(name) if assignment_op(self.peek()).is_some() => {
                let name = name.clone();
                self.parse_var_store_stmt(name, true)
            }
            TokenKind::Eof => Err(EvalParseError::UnexpectedEof),
            _ => {
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Returns true when the current tokens form `Class::$property <assign-op>`.
    pub(super) fn current_starts_static_property_assignment(&self) -> bool {
        let mut pos = self.pos;
        if matches!(self.tokens.get(pos), Some(TokenKind::Backslash)) {
            pos += 1;
        }
        if !matches!(self.tokens.get(pos), Some(TokenKind::Ident(_))) {
            return false;
        }
        pos += 1;
        while matches!(self.tokens.get(pos), Some(TokenKind::Backslash)) {
            pos += 1;
            if !matches!(self.tokens.get(pos), Some(TokenKind::Ident(_))) {
                return false;
            }
            pos += 1;
        }
        if !matches!(self.tokens.get(pos), Some(TokenKind::DoubleColon)) {
            return false;
        }
        pos += 1;
        let Some(TokenKind::DollarIdent(_)) = self.tokens.get(pos) else {
            return false;
        };
        pos += 1;
        self.tokens
            .get(pos)
            .is_some_and(|token| assignment_op(token).is_some())
    }

    /// Parses `do { ... } while (expr);`.
    pub(super) fn parse_do_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let body = self.parse_statement_body()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "while")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::DoWhile { body, condition }])
    }

    /// Parses `$name[index] = expr;` and `$name[] = expr;` eval writes.
    pub(super) fn parse_array_set_stmt(
        &mut self,
        name: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            self.expect_semicolon()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
    }

    /// Parses `for (init; condition; update) { ... }`.
    pub(super) fn parse_for_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let init = self.parse_for_init_clause()?;
        self.expect_semicolon()?;
        let condition = if matches!(self.current(), TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_semicolon()?;
        let update = self.parse_for_update_clause()?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::For {
            init,
            condition,
            update,
            body,
        }])
    }

    /// Parses `foreach (expr as $value) { ... }` or `foreach (expr as $key => $value) { ... }`.
    pub(super) fn parse_foreach_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let array = self.parse_expr()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "as")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        let TokenKind::DollarIdent(value_name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let value_name = value_name.clone();
        self.advance();
        let (key_name, value_name) = if matches!(self.current(), TokenKind::FatArrow) {
            self.advance();
            let TokenKind::DollarIdent(next_value_name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let key_name = value_name;
            let value_name = next_value_name.clone();
            self.advance();
            (Some(key_name), value_name)
        } else {
            (None, value_name)
        };
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        }])
    }

    /// Parses `[abstract|final] class Name [extends Parent] [implements Iface, ...] { ... }`.
    pub(super) fn parse_class_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let (is_abstract, is_final) = self.parse_class_decl_modifiers()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "class")) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        let parent = self.parse_class_parent_clause()?;
        let interfaces = self.parse_class_interface_clause()?;
        self.expect(TokenKind::LBrace)?;
        let mut constants = Vec::new();
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        let mut traits = Vec::new();
        let mut trait_adaptations = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_class_member(
                &mut constants,
                &mut properties,
                &mut methods,
                &mut traits,
                &mut trait_adaptations,
            )?;
        }
        self.consume_semicolon();
        Ok(vec![EvalStmt::ClassDecl(
            EvalClass::with_modifiers_traits_adaptations_and_constants(
                name,
                is_abstract,
                is_final,
                parent,
                interfaces,
                traits,
                trait_adaptations,
                constants,
                properties,
                methods,
            ),
        )])
    }

    /// Parses class-level `abstract` and `final` modifiers before `class`.
    pub(super) fn parse_class_decl_modifiers(&mut self) -> Result<(bool, bool), EvalParseError> {
        let mut is_abstract = false;
        let mut is_final = false;
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
                _ => return Ok((is_abstract, is_final)),
            }
        }
    }

    /// Parses an optional `extends Parent` class declaration clause.
    pub(super) fn parse_class_parent_clause(&mut self) -> Result<Option<String>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "extends")) {
            return Ok(None);
        }
        self.advance();
        let parent = self.parse_qualified_name()?;
        Ok(Some(self.resolve_class_name(parent)))
    }

    /// Parses an optional `implements Iface, ...` class declaration clause.
    pub(super) fn parse_class_interface_clause(&mut self) -> Result<Vec<String>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "implements")) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut interfaces = Vec::new();
        loop {
            let interface = self.parse_qualified_name()?;
            interfaces.push(self.resolve_class_name(interface));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        Ok(interfaces)
    }

    /// Parses one public property or method from an eval class body.
    pub(super) fn parse_class_member(
        &mut self,
        constants: &mut Vec<EvalClassConstant>,
        properties: &mut Vec<EvalClassProperty>,
        methods: &mut Vec<EvalClassMethod>,
        traits: &mut Vec<String>,
        trait_adaptations: &mut Vec<EvalTraitAdaptation>,
    ) -> Result<(), EvalParseError> {
        let (visibility, is_static, is_abstract, is_final) = self.parse_class_member_modifiers()?;

        if is_abstract && is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }

        if visibility.is_none()
            && !is_static
            && !is_abstract
            && !is_final
            && matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "use"))
        {
            self.parse_class_trait_use(traits, trait_adaptations)?;
            return Ok(());
        }

        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || is_abstract || is_final {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants
                .push(self.parse_class_const_decl(visibility.unwrap_or(EvalVisibility::Public))?);
            return Ok(());
        }

        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            methods.push(self.parse_class_method_decl(
                visibility.unwrap_or(EvalVisibility::Public),
                is_static,
                is_abstract,
                is_final,
            )?);
            return Ok(());
        }

        let visibility = visibility.unwrap_or(EvalVisibility::Public);
        if is_abstract || is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        properties.push(self.parse_class_property_decl(visibility, is_static)?);
        Ok(())
    }

    /// Parses one eval class constant declaration.
    pub(super) fn parse_class_const_decl(
        &mut self,
        visibility: EvalVisibility,
    ) -> Result<EvalClassConstant, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(EvalClassConstant::with_visibility(name, visibility, value))
    }

    /// Parses `use TraitName, OtherTrait;` or an adaptation block inside an eval class body.
    pub(super) fn parse_class_trait_use(
        &mut self,
        traits: &mut Vec<String>,
        trait_adaptations: &mut Vec<EvalTraitAdaptation>,
    ) -> Result<(), EvalParseError> {
        self.advance();
        loop {
            let trait_name = self.parse_qualified_name()?;
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
    pub(super) fn parse_trait_adaptation(&mut self) -> Result<EvalTraitAdaptation, EvalParseError> {
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
                    let trait_name = self.parse_qualified_name()?;
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
    pub(super) fn parse_trait_adaptation_target(
        &mut self,
    ) -> Result<(Option<String>, String), EvalParseError> {
        let first = self.parse_qualified_name()?;
        if self.consume(TokenKind::DoubleColon) {
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
    pub(super) fn parse_optional_trait_adaptation_visibility(
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
    pub(super) fn parse_class_member_modifiers(
        &mut self,
    ) -> Result<(Option<EvalVisibility>, bool, bool, bool), EvalParseError> {
        let mut visibility = None;
        let mut is_static = false;
        let mut is_abstract = false;
        let mut is_final = false;
        loop {
            match self.current() {
                TokenKind::Ident(name) if ident_eq(name, "public") => {
                    if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    visibility = Some(EvalVisibility::Public);
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "protected") => {
                    if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    visibility = Some(EvalVisibility::Protected);
                    self.advance();
                }
                TokenKind::Ident(name) if ident_eq(name, "private") => {
                    if visibility.is_some() {
                        return Err(EvalParseError::UnsupportedConstruct);
                    }
                    visibility = Some(EvalVisibility::Private);
                    self.advance();
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
                TokenKind::Ident(name) if is_unsupported_class_member_modifier(name) => {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                _ => return Ok((visibility, is_static, is_abstract, is_final)),
            }
        }
    }

    /// Parses `function name($param, ...) { ... }` or an abstract method signature.
    pub(super) fn parse_class_method_decl(
        &mut self,
        visibility: EvalVisibility,
        is_static: bool,
        is_abstract: bool,
        is_final: bool,
    ) -> Result<EvalClassMethod, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        let body = if is_abstract {
            self.expect_semicolon()?;
            Vec::new()
        } else {
            self.parse_block()?
        };
        Ok(EvalClassMethod::with_visibility_and_modifiers(
            name,
            visibility,
            is_static,
            is_abstract,
            is_final,
            params,
            body,
        ))
    }

    /// Parses one public property declaration with an optional initializer.
    pub(super) fn parse_class_property_decl(
        &mut self,
        visibility: EvalVisibility,
        is_static: bool,
    ) -> Result<EvalClassProperty, EvalParseError> {
        self.skip_optional_property_type()?;
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        let default = if self.consume(TokenKind::Equal) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_semicolon()?;
        Ok(EvalClassProperty::with_visibility_and_static(
            name, visibility, is_static, default,
        ))
    }

    /// Parses `trait Name { ... }` declarations into dynamic trait metadata.
    pub(super) fn parse_trait_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        self.expect(TokenKind::LBrace)?;
        let mut constants = Vec::new();
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_trait_member(&mut constants, &mut properties, &mut methods)?;
        }
        self.consume_semicolon();
        Ok(vec![EvalStmt::TraitDecl(EvalTrait::with_constants(
            name, constants, properties, methods,
        ))])
    }

    /// Parses one property or method from an eval trait body.
    pub(super) fn parse_trait_member(
        &mut self,
        constants: &mut Vec<EvalClassConstant>,
        properties: &mut Vec<EvalClassProperty>,
        methods: &mut Vec<EvalClassMethod>,
    ) -> Result<(), EvalParseError> {
        let (visibility, is_static, is_abstract, is_final) = self.parse_class_member_modifiers()?;
        if is_abstract && is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || is_abstract || is_final {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants
                .push(self.parse_class_const_decl(visibility.unwrap_or(EvalVisibility::Public))?);
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            methods.push(self.parse_class_method_decl(
                visibility.unwrap_or(EvalVisibility::Public),
                is_static,
                is_abstract,
                is_final,
            )?);
            return Ok(());
        }
        let visibility = visibility.unwrap_or(EvalVisibility::Public);
        if is_abstract || is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        properties.push(self.parse_class_property_decl(visibility, is_static)?);
        Ok(())
    }

    /// Parses `enum Name [: int|string] [implements Iface, ...] { ... }`.
    pub(super) fn parse_enum_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        let backing_type = self.parse_enum_backing_type()?;
        let interfaces = self.parse_class_interface_clause()?;
        self.expect(TokenKind::LBrace)?;
        let mut cases = Vec::new();
        let mut constants = Vec::new();
        let mut methods = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_enum_member(&mut cases, &mut constants, &mut methods)?;
        }
        self.consume_semicolon();
        Ok(vec![EvalStmt::EnumDecl(EvalEnum::with_members(
            name,
            backing_type,
            interfaces,
            cases,
            constants,
            methods,
        ))])
    }

    /// Parses an optional backed-enum scalar type after the enum name.
    pub(super) fn parse_enum_backing_type(
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
    pub(super) fn parse_enum_member(
        &mut self,
        cases: &mut Vec<EvalEnumCase>,
        constants: &mut Vec<EvalClassConstant>,
        methods: &mut Vec<EvalClassMethod>,
    ) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "case")) {
            cases.push(self.parse_enum_case_decl()?);
            return Ok(());
        }
        let (visibility, is_static, is_abstract, is_final) = self.parse_class_member_modifiers()?;
        if is_abstract {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || is_final {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants
                .push(self.parse_class_const_decl(visibility.unwrap_or(EvalVisibility::Public))?);
            return Ok(());
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            methods.push(self.parse_class_method_decl(
                visibility.unwrap_or(EvalVisibility::Public),
                is_static,
                false,
                is_final,
            )?);
            return Ok(());
        }
        Err(EvalParseError::UnsupportedConstruct)
    }

    /// Parses `case Name;` or `case Name = expr;` inside an eval enum body.
    pub(super) fn parse_enum_case_decl(&mut self) -> Result<EvalEnumCase, EvalParseError> {
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

    /// Parses `interface Name [extends Parent, ...] { function name(...); }`.
    pub(super) fn parse_interface_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        let parents = self.parse_interface_parent_clause()?;
        self.expect(TokenKind::LBrace)?;
        let mut constants = Vec::new();
        let mut methods = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            if let Some(constant) = self.parse_optional_interface_constant_decl()? {
                constants.push(constant);
            } else {
                methods.push(self.parse_interface_method_decl()?);
            }
        }
        self.consume_semicolon();
        Ok(vec![EvalStmt::InterfaceDecl(
            EvalInterface::with_constants(name, parents, constants, methods),
        )])
    }

    /// Parses an optional `extends Parent, ...` interface declaration clause.
    pub(super) fn parse_interface_parent_clause(&mut self) -> Result<Vec<String>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "extends")) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut parents = Vec::new();
        loop {
            let parent = self.parse_qualified_name()?;
            parents.push(self.resolve_class_name(parent));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        Ok(parents)
    }

    /// Parses an interface constant declaration when the current member starts with `const`.
    pub(super) fn parse_optional_interface_constant_decl(
        &mut self,
    ) -> Result<Option<EvalClassConstant>, EvalParseError> {
        let visibility = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "public"))
        {
            self.advance();
            EvalVisibility::Public
        } else {
            EvalVisibility::Public
        };
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            return Ok(None);
        }
        Ok(Some(self.parse_class_const_decl(visibility)?))
    }

    /// Parses one eval interface method signature.
    pub(super) fn parse_interface_method_decl(
        &mut self,
    ) -> Result<EvalInterfaceMethod, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "public")) {
            self.advance();
        } else if matches!(self.current(), TokenKind::Ident(name) if is_unsupported_class_member_modifier(name))
        {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        self.expect_semicolon()?;
        Ok(EvalInterfaceMethod::new(name, params))
    }

    /// Consumes a simple declared property type before the `$property` token.
    pub(super) fn skip_optional_property_type(&mut self) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::DollarIdent(_)) {
            return Ok(());
        }
        if self.consume(TokenKind::Question) && matches!(self.current(), TokenKind::DollarIdent(_))
        {
            return Err(EvalParseError::UnexpectedToken);
        }
        match self.current() {
            TokenKind::Ident(_) | TokenKind::Backslash => {
                let _ = self.parse_qualified_name()?;
                Ok(())
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Parses `function name($param, ...) { ... }` declarations.
    pub(super) fn parse_function_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        let body = self.parse_block()?;
        Ok(vec![EvalStmt::FunctionDecl { name, params, body }])
    }

    /// Parses `namespace Name;` or `namespace Name { ... }` eval namespace blocks.
    pub(super) fn parse_namespace_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let namespace = if self.consume(TokenKind::LBrace) {
            return self.parse_namespace_block(String::new());
        } else {
            self.parse_namespace_name()?
        };
        if self.consume_semicolon() {
            self.namespace = namespace;
            self.imports = NamespaceImports::default();
            return Ok(Vec::new());
        }
        self.expect(TokenKind::LBrace)?;
        self.parse_namespace_block(namespace)
    }

    /// Parses statements inside an already opened namespace block.
    pub(super) fn parse_namespace_block(
        &mut self,
        namespace: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.namespace, namespace);
        let previous_imports = std::mem::take(&mut self.imports);
        let previous_allow_use_imports = std::mem::replace(&mut self.allow_use_imports, true);
        let result = self.parse_block_contents();
        self.namespace = previous;
        self.imports = previous_imports;
        self.allow_use_imports = previous_allow_use_imports;
        result
    }

    /// Parses a namespace declaration name without a leading global separator.
    pub(super) fn parse_namespace_name(&mut self) -> Result<String, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if name.absolute {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(name.name)
    }

    /// Parses PHP `use`, `use function`, and `use const` import declarations.
    pub(super) fn parse_use_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let kind = self.parse_use_import_kind();

        loop {
            self.parse_use_import(kind)?;
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(Vec::new())
    }

    /// Parses an optional top-level `function` or `const` use-import kind.
    pub(super) fn parse_use_import_kind(&mut self) -> UseImportKind {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            self.advance();
            UseImportKind::Function
        } else if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            self.advance();
            UseImportKind::Const
        } else {
            UseImportKind::Class
        }
    }

    /// Parses and registers one comma-separated import entry.
    pub(super) fn parse_use_import(&mut self, kind: UseImportKind) -> Result<(), EvalParseError> {
        let (name, grouped) = self.parse_use_name_or_group_start()?;
        if grouped {
            return self.parse_grouped_use_imports(kind, name);
        }
        self.parse_use_alias_and_register(kind, name)
    }

    /// Parses a use-import name, stopping after a trailing namespace separator before `{`.
    pub(super) fn parse_use_name_or_group_start(
        &mut self,
    ) -> Result<(String, bool), EvalParseError> {
        let _ = self.consume(TokenKind::Backslash);
        let TokenKind::Ident(first) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let mut name = first.clone();
        self.advance();
        while self.consume(TokenKind::Backslash) {
            if self.consume(TokenKind::LBrace) {
                return Ok((name, true));
            }
            let TokenKind::Ident(part) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            name.push('\\');
            name.push_str(part);
            self.advance();
        }
        Ok((name, false))
    }

    /// Parses all members inside a grouped namespace import declaration.
    pub(super) fn parse_grouped_use_imports(
        &mut self,
        default_kind: UseImportKind,
        prefix: String,
    ) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::RBrace) {
            return Err(EvalParseError::UnexpectedToken);
        }
        loop {
            let kind = self.parse_grouped_use_entry_kind(default_kind)?;
            let member = self.parse_grouped_use_member_name()?;
            let name = join_grouped_use_name(&prefix, &member);
            self.parse_use_alias_and_register(kind, name)?;
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RBrace) {
                return Ok(());
            }
        }
        self.expect(TokenKind::RBrace)
    }

    /// Parses an optional per-entry grouped import kind, matching PHP's mixed group rules.
    pub(super) fn parse_grouped_use_entry_kind(
        &mut self,
        default_kind: UseImportKind,
    ) -> Result<UseImportKind, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            if default_kind != UseImportKind::Class {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.advance();
            return Ok(UseImportKind::Function);
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if default_kind != UseImportKind::Class {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.advance();
            return Ok(UseImportKind::Const);
        }
        Ok(default_kind)
    }

    /// Parses one non-absolute member name inside a grouped use declaration.
    pub(super) fn parse_grouped_use_member_name(&mut self) -> Result<String, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if name.absolute {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(name.name)
    }

    /// Parses an optional alias and stores one namespace import.
    pub(super) fn parse_use_alias_and_register(
        &mut self,
        kind: UseImportKind,
        name: String,
    ) -> Result<(), EvalParseError> {
        let alias = if matches!(
            self.current(),
            TokenKind::Ident(keyword) if ident_eq(keyword, "as")
        ) {
            self.advance();
            let TokenKind::Ident(alias) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let alias = alias.clone();
            self.advance();
            alias
        } else {
            last_name_segment(&name).to_string()
        };

        match kind {
            UseImportKind::Class => self.imports.insert_class(alias, name),
            UseImportKind::Function => self.imports.insert_function(alias, name),
            UseImportKind::Const => self.imports.insert_constant(alias, name),
        }
        Ok(())
    }

    /// Parses `global $name, $other;` declarations in eval fragments.
    pub(super) fn parse_global_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
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

    /// Parses `static $name = expr;` declarations in eval fragments.
    pub(super) fn parse_static_var_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::Equal)?;
        let init = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::StaticVar { name, init }])
    }

    /// Parses `throw expr;` statements in eval fragments.
    pub(super) fn parse_throw_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let expr = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Throw(expr)])
    }

    /// Parses `try { ... } catch (Type|Other $name) { ... } finally { ... }` statements.
    pub(super) fn parse_try_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let body = self.parse_block()?;
        let mut catches = Vec::new();
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "catch")) {
            catches.push(self.parse_catch_clause()?);
        }
        let finally_body = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "finally"))
        {
            self.advance();
            self.parse_block()?
        } else {
            Vec::new()
        };
        if catches.is_empty() && finally_body.is_empty() {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(vec![EvalStmt::Try {
            body,
            catches,
            finally_body,
        }])
    }

    /// Parses one `catch (ClassName|Other [$name]) { ... }` clause.
    pub(super) fn parse_catch_clause(&mut self) -> Result<EvalCatch, EvalParseError> {
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
    pub(super) fn parse_catch_types(&mut self) -> Result<Vec<String>, EvalParseError> {
        let class_name = self.parse_qualified_name()?;
        let mut class_names = vec![self.resolve_class_name(class_name)];
        while self.consume(TokenKind::Pipe) {
            let class_name = self.parse_qualified_name()?;
            class_names.push(self.resolve_class_name(class_name));
        }
        Ok(class_names)
    }

    /// Parses a dynamic function declaration parameter list after `(`.
    pub(super) fn parse_function_params(&mut self) -> Result<Vec<String>, EvalParseError> {
        let mut params = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            params.push(name.clone());
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if matches!(self.current(), TokenKind::RParen) {
                return Err(EvalParseError::ExpectedVariable);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    /// Parses the optional first clause of a `for` loop.
    pub(super) fn parse_for_init_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Semicolon) {
            return Ok(Vec::new());
        }
        self.parse_for_clause_stmt()
    }

    /// Parses the optional update clause of a `for` loop.
    pub(super) fn parse_for_update_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if self.consume(TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let statements = self.parse_for_clause_stmt()?;
        self.expect(TokenKind::RParen)?;
        Ok(statements)
    }

    /// Parses one statement-like `for` clause without consuming a delimiter.
    pub(super) fn parse_for_clause_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::PlusPlus | TokenKind::MinusMinus => self.parse_prefix_inc_dec_stmt(false),
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_clause(name.clone())
            }
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(false)
            }
            TokenKind::DollarIdent(name)
                if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) =>
            {
                self.parse_postfix_inc_dec_stmt(name.clone(), false)
            }
            TokenKind::DollarIdent(name) if assignment_op(self.peek()).is_some() => {
                let name = name.clone();
                self.parse_var_store_stmt(name, false)
            }
            _ => {
                let expr = self.parse_expr()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Parses `$name[index] = expr` and `$name[] = expr` in a `for` clause.
    pub(super) fn parse_array_set_clause(
        &mut self,
        name: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
    }

    /// Parses `$name = expr` and simple variable compound assignments.
    pub(super) fn parse_var_store_stmt(
        &mut self,
        name: String,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && matches!(self.current(), TokenKind::Ampersand) {
            self.advance();
            let TokenKind::DollarIdent(source) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let source = source.clone();
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::ReferenceAssign {
                target: name,
                source,
            }]);
        }
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        let value = assignment_value(&name, op, value);
        Ok(vec![EvalStmt::StoreVar { name, value }])
    }

    /// Parses `Class::$property = expr` and simple static-property compound assignments.
    pub(super) fn parse_static_property_set_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let class_name = self.parse_qualified_name()?;
        let class_name = self.resolve_static_class_name(class_name);
        self.expect(TokenKind::DoubleColon)?;
        let TokenKind::DollarIdent(property) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let property = property.clone();
        self.advance();
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        let value = match op {
            Some(op) => EvalExpr::Binary {
                op,
                left: Box::new(EvalExpr::StaticPropertyGet {
                    class_name: class_name.clone(),
                    property: property.clone(),
                }),
                right: Box::new(value),
            },
            None => value,
        };
        Ok(vec![EvalStmt::StaticPropertySet {
            class_name,
            property,
            value,
        }])
    }

    /// Parses prefix `++$name` and `--$name` as simple statement effects.
    pub(super) fn parse_prefix_inc_dec_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![inc_dec_store(name, increment)])
    }

    /// Parses postfix `$name++` and `$name--` as simple statement effects.
    pub(super) fn parse_postfix_inc_dec_stmt(
        &mut self,
        name: String,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![inc_dec_store(name, increment)])
    }

    /// Parses `$object->property` as either an expression statement or property write.
    pub(super) fn parse_property_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let target = self.parse_expr()?;
        if !self.consume(TokenKind::Equal) {
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::Expr(target)]);
        }
        let EvalExpr::PropertyGet { object, property } = target else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![EvalStmt::PropertySet {
            object: *object,
            property,
            value,
        }])
    }

    /// Parses a complete `if` statement after consuming the `if` keyword.
    pub(super) fn parse_if_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        Ok(vec![self.parse_if_after_keyword()?])
    }

    /// Parses the condition, then block, and optional else branch for an `if` chain.
    pub(super) fn parse_if_after_keyword(&mut self) -> Result<EvalStmt, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_statement_body()?;
        let else_branch = self.parse_optional_else_branch()?;
        Ok(EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Parses `elseif`, `else if`, or `else` branches after an `if` body.
    pub(super) fn parse_optional_else_branch(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "elseif")) {
            self.advance();
            return Ok(vec![self.parse_if_after_keyword()?]);
        }
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "else")) {
            return Ok(Vec::new());
        }
        self.advance();
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "if")) {
            self.advance();
            Ok(vec![self.parse_if_after_keyword()?])
        } else {
            self.parse_statement_body()
        }
    }

    /// Parses `switch (expr) { case expr: ... default: ... }`.
    pub(super) fn parse_switch_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut cases = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            cases.push(self.parse_switch_case()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(vec![EvalStmt::Switch { expr, cases }])
    }

    /// Parses one `case` or `default` arm inside a switch body.
    pub(super) fn parse_switch_case(&mut self) -> Result<EvalSwitchCase, EvalParseError> {
        let condition = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "case"))
        {
            self.advance();
            let condition = self.parse_expr()?;
            Some(condition)
        } else if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "default")) {
            self.advance();
            None
        } else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.expect(TokenKind::Colon)?;
        let body = self.parse_switch_case_body()?;
        Ok(EvalSwitchCase { condition, body })
    }

    /// Parses case body statements until the next case boundary or switch close.
    pub(super) fn parse_switch_case_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut body = Vec::new();
        while !is_switch_case_boundary(self.current()) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            body.extend(self.parse_stmt()?);
        }
        Ok(body)
    }

    /// Parses `unset($name[, ...]);`.
    pub(super) fn parse_unset_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let mut statements = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            statements.push(EvalStmt::UnsetVar { name: name.clone() });
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        Ok(statements)
    }

    /// Parses `while (expr) { ... }`.
    pub(super) fn parse_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::While { condition, body }])
    }

    /// Parses either a brace-delimited block or one braceless statement body.
    pub(super) fn parse_statement_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.parse_nested_stmt()
        }
    }

    /// Parses a brace-delimited statement block.
    pub(super) fn parse_block(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        self.parse_nested_block_contents()
    }

    /// Parses one nested statement where import declarations are not legal.
    pub(super) fn parse_nested_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_stmt();
        self.allow_use_imports = previous;
        result
    }

    /// Parses a nested block while preserving active imports for name resolution.
    pub(super) fn parse_nested_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_block_contents();
        self.allow_use_imports = previous;
        result
    }

    /// Parses statements until the closing brace for the current block.
    pub(super) fn parse_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            statements.extend(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(statements)
    }
}
