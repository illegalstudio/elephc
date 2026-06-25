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
    EvalArrayElement, EvalAttribute, EvalAttributeArg, EvalCallArg, EvalCatch, EvalClass,
    EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalConst, EvalEnum,
    EvalEnumBackingType, EvalEnumCase, EvalExpr, EvalInstanceOfTarget, EvalInterface,
    EvalInterfaceMethod, EvalInterfaceProperty, EvalParameterType, EvalParameterTypeVariant,
    EvalSourceLocation, EvalStmt, EvalSwitchCase, EvalTrait, EvalTraitAdaptation, EvalUnaryOp,
    EvalVisibility,
};
use crate::lexer::TokenKind;

/// Parsed method parameters plus constructor-promotion side products.
pub(super) struct ParsedMethodParams {
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    promoted_properties: Vec<EvalClassProperty>,
    promoted_assignments: Vec<EvalStmt>,
}

/// Class-body members collected while parsing a named or anonymous eval class.
struct ParsedClassBody {
    source_end_line: i64,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
}

/// Type-declaration position controls PHP-only atoms such as `void`, `static`, and `callable`.
#[derive(Clone, Copy)]
enum EvalTypePosition {
    FunctionParameter,
    MethodParameter,
    Property,
    FunctionReturn,
    MethodReturn,
}

impl Parser {
    /// Parses one source statement, expanding `unset($a, $b)` to one statement per variable.
    pub(super) fn parse_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::AttributeStart) {
            return self.parse_attributed_stmt();
        }
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
            TokenKind::Ident(name)
                if ident_eq(name, "abstract")
                    || ident_eq(name, "final")
                    || ident_eq(name, "readonly") =>
            {
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

    /// Parses one declaration preceded by PHP attribute groups.
    pub(super) fn parse_attributed_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let attributes = self.parse_attribute_groups()?;
        match self.current() {
            TokenKind::Ident(name)
                if ident_eq(name, "abstract")
                    || ident_eq(name, "final")
                    || ident_eq(name, "readonly")
                    || ident_eq(name, "class") =>
            {
                self.parse_class_decl_stmt_with_attributes(attributes)
            }
            TokenKind::Ident(name) if ident_eq(name, "enum") => {
                self.parse_enum_decl_stmt_with_attributes(attributes)
            }
            TokenKind::Ident(name) if ident_eq(name, "interface") => {
                self.parse_interface_decl_stmt_with_attributes(attributes)
            }
            TokenKind::Ident(name) if ident_eq(name, "trait") => {
                self.parse_trait_decl_stmt_with_attributes(attributes)
            }
            TokenKind::Ident(name) if ident_eq(name, "function") => {
                self.parse_function_decl_stmt_with_attributes(attributes)
            }
            _ => Err(EvalParseError::UnsupportedConstruct),
        }
    }

    /// Parses one or more PHP `#[...]` attribute groups.
    pub(super) fn parse_attribute_groups(&mut self) -> Result<Vec<EvalAttribute>, EvalParseError> {
        let mut attributes = Vec::new();
        while self.consume(TokenKind::AttributeStart) {
            loop {
                attributes.push(self.parse_attribute()?);
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBracket)?;
        }
        Ok(attributes)
    }

    /// Parses one attribute name and optional literal positional/named arguments.
    pub(super) fn parse_attribute(&mut self) -> Result<EvalAttribute, EvalParseError> {
        let name = self.parse_qualified_name()?;
        let name = self.resolve_class_name(name);
        let args = if self.consume(TokenKind::LParen) {
            let mut args = Vec::new();
            let mut supported = true;
            if !self.consume(TokenKind::RParen) {
                loop {
                    let call_arg = self.parse_call_arg()?;
                    if supported {
                        if call_arg.is_spread() {
                            supported = false;
                        } else if let Some(arg) = eval_attribute_arg_from_expr(call_arg.value()) {
                            args.push(match call_arg.name() {
                                Some(name) => EvalAttributeArg::Named {
                                    name: name.to_string(),
                                    value: Box::new(arg),
                                },
                                None => arg,
                            });
                        } else {
                            supported = false;
                        }
                    }
                    if self.consume(TokenKind::RParen) {
                        break;
                    }
                    self.expect(TokenKind::Comma)?;
                }
            }
            supported.then_some(args)
        } else {
            Some(Vec::new())
        };
        Ok(EvalAttribute::new(name, args))
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

    /// Parses `[abstract|final|readonly] class Name [extends Parent] [implements Iface, ...] { ... }`.
    pub(super) fn parse_class_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_class_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses a class declaration and attaches already parsed class attributes.
    pub(super) fn parse_class_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let (is_abstract, is_final, is_readonly_class) = self.parse_class_decl_modifiers()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "class")) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let source_start_line = self.current_line();
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
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
    pub(super) fn parse_anonymous_class_expr(
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

    /// Parses members inside a class body after relation clauses.
    fn parse_class_body_members(
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
    pub(super) fn parse_class_decl_modifiers(
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

        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || is_abstract || is_readonly || set_visibility.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants.push(
                self.parse_class_const_decl(
                    visibility.unwrap_or(EvalVisibility::Public),
                    is_final,
                )?
                .with_attributes(attributes),
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
        let (property, mut hook_methods) = self.parse_class_property_decl(
            visibility,
            set_visibility,
            is_static,
            is_final,
            is_readonly,
            is_readonly_class,
            is_abstract,
        )?;
        properties.push(property.with_attributes(attributes));
        methods.append(&mut hook_methods);
        Ok(())
    }

    /// Parses optional attributes that decorate one class-like member.
    pub(super) fn parse_optional_member_attributes(
        &mut self,
    ) -> Result<Vec<EvalAttribute>, EvalParseError> {
        if matches!(self.current(), TokenKind::AttributeStart) {
            self.parse_attribute_groups()
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses one eval class constant declaration.
    pub(super) fn parse_class_const_decl(
        &mut self,
        visibility: EvalVisibility,
        is_final: bool,
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
        Ok(EvalClassConstant::with_visibility_and_final(
            name, visibility, is_final, value,
        ))
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
    fn consume_set_marker(&mut self) -> Result<bool, EvalParseError> {
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
    fn eval_visibility_rank(visibility: EvalVisibility) -> u8 {
        match visibility {
            EvalVisibility::Private => 1,
            EvalVisibility::Protected => 2,
            EvalVisibility::Public => 3,
        }
    }

    /// Parses one class/trait/enum method and returns constructor-promoted properties.
    pub(super) fn parse_class_method_decl(
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

    /// Parses one public property declaration with an optional initializer.
    pub(super) fn parse_class_property_decl(
        &mut self,
        visibility: EvalVisibility,
        set_visibility: Option<EvalVisibility>,
        is_static: bool,
        is_final: bool,
        is_readonly: bool,
        is_readonly_class: bool,
        is_abstract: bool,
    ) -> Result<(EvalClassProperty, Vec<EvalClassMethod>), EvalParseError> {
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
            let (requires_get_hook, requires_set_hook) = self.parse_property_hook_contracts()?;
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
            return Ok((property, Vec::new()));
        }
        let default_is_some = default.is_some();
        let (has_get_hook, has_set_hook, hook_methods) =
            self.parse_property_hook_tail(&name, is_static, effective_readonly, default_is_some)?;
        let is_virtual = (has_get_hook || has_set_hook)
            && !property_hook_methods_use_backing_slot(&hook_methods, &name);
        let property = EvalClassProperty::with_visibility_static_final_and_readonly(
            name,
            visibility,
            is_static,
            is_final,
            effective_readonly,
            default,
        )
        .with_type(property_type)
        .with_set_visibility(set_visibility)
        .with_hooks(has_get_hook, has_set_hook)
        .with_virtual(is_virtual);
        Ok((property, hook_methods))
    }

    /// Parses `;` or a concrete eval property hook block after one property declaration.
    pub(super) fn parse_property_hook_tail(
        &mut self,
        property_name: &str,
        is_static: bool,
        is_readonly: bool,
        has_default: bool,
    ) -> Result<(bool, bool, Vec<EvalClassMethod>), EvalParseError> {
        if self.consume(TokenKind::Semicolon) {
            return Ok((false, false, Vec::new()));
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
        let mut methods = Vec::new();
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            let (is_get, method) = self.parse_property_hook_decl(property_name)?;
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
            }
            methods.push(method);
        }
        if !has_get_hook && !has_set_hook {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok((has_get_hook, has_set_hook, methods))
    }

    /// Parses one concrete `get` or `set` property hook declaration.
    pub(super) fn parse_property_hook_decl(
        &mut self,
        property_name: &str,
    ) -> Result<(bool, EvalClassMethod), EvalParseError> {
        if self.consume(TokenKind::Ampersand) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let TokenKind::Ident(hook_name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let is_get = ident_eq(hook_name, "get");
        let is_set = ident_eq(hook_name, "set");
        if !is_get && !is_set {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        let source_start_line = self.current_line();
        self.advance();
        let params = if is_set {
            vec![self.parse_property_set_hook_param()?]
        } else {
            Vec::new()
        };
        let (body, source_end_line) = match self.current() {
            TokenKind::Semicolon => return Err(EvalParseError::UnsupportedConstruct),
            TokenKind::FatArrow if is_get => {
                self.advance();
                let expr = self.parse_expr()?;
                let source_end_line = self.current_line();
                self.expect_semicolon()?;
                (vec![EvalStmt::Return(Some(expr))], source_end_line)
            }
            TokenKind::FatArrow => return Err(EvalParseError::UnsupportedConstruct),
            TokenKind::LBrace => self.parse_block_with_end_line()?,
            _ => return Err(EvalParseError::UnexpectedToken),
        };
        let method_name = if is_get {
            property_hook_get_method(property_name)
        } else {
            property_hook_set_method(property_name)
        };
        Ok((
            is_get,
            EvalClassMethod::with_visibility_and_modifiers(
                method_name,
                EvalVisibility::Public,
                false,
                false,
                false,
                params,
                body,
            )
            .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line)),
        ))
    }

    /// Parses an optional set-hook parameter list and returns the hook value variable.
    pub(super) fn parse_property_set_hook_param(&mut self) -> Result<String, EvalParseError> {
        if !self.consume(TokenKind::LParen) {
            return Ok("value".to_string());
        }
        let _ = self.parse_optional_property_type()?;
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::RParen)?;
        Ok(name)
    }

    /// Parses `trait Name { ... }` declarations into dynamic trait metadata.
    pub(super) fn parse_trait_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_trait_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses a trait declaration and attaches already parsed class-like attributes.
    pub(super) fn parse_trait_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
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
        let source_end_line = loop {
            if matches!(self.current(), TokenKind::RBrace) {
                let source_end_line = self.current_line();
                self.advance();
                break source_end_line;
            }
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_trait_member(&mut constants, &mut properties, &mut methods)?;
        };
        self.consume_semicolon();
        Ok(vec![EvalStmt::TraitDecl(
            EvalTrait::with_constants(name, constants, properties, methods)
                .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
                .with_attributes(attributes),
        )])
    }

    /// Parses one property or method from an eval trait body.
    pub(super) fn parse_trait_member(
        &mut self,
        constants: &mut Vec<EvalClassConstant>,
        properties: &mut Vec<EvalClassProperty>,
        methods: &mut Vec<EvalClassMethod>,
    ) -> Result<(), EvalParseError> {
        let attributes = self.parse_optional_member_attributes()?;
        let (visibility, set_visibility, is_static, is_abstract, is_final, is_readonly) =
            self.parse_class_member_modifiers()?;
        if is_abstract && is_final {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static || is_abstract || is_readonly || set_visibility.is_some() {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants.push(
                self.parse_class_const_decl(
                    visibility.unwrap_or(EvalVisibility::Public),
                    is_final,
                )?
                .with_attributes(attributes),
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
        let (property, mut hook_methods) = self.parse_class_property_decl(
            visibility,
            set_visibility,
            is_static,
            is_final,
            is_readonly,
            false,
            is_abstract,
        )?;
        properties.push(property.with_attributes(attributes));
        methods.append(&mut hook_methods);
        Ok(())
    }

    /// Parses `enum Name [: int|string] [implements Iface, ...] { ... }`.
    pub(super) fn parse_enum_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_enum_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses an enum declaration and attaches already parsed class-like attributes.
    pub(super) fn parse_enum_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
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
        let source_end_line = loop {
            if matches!(self.current(), TokenKind::RBrace) {
                let source_end_line = self.current_line();
                self.advance();
                break source_end_line;
            }
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            self.parse_enum_member(&mut cases, &mut constants, &mut methods)?;
        };
        self.consume_semicolon();
        Ok(vec![EvalStmt::EnumDecl(
            EvalEnum::with_members(name, backing_type, interfaces, cases, constants, methods)
                .with_source_location(EvalSourceLocation::new(source_start_line, source_end_line))
                .with_attributes(attributes),
        )])
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
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if is_static {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            constants.push(
                self.parse_class_const_decl(
                    visibility.unwrap_or(EvalVisibility::Public),
                    is_final,
                )?
                .with_attributes(attributes),
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
        self.parse_interface_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses an interface declaration and attaches already parsed class-like attributes.
    pub(super) fn parse_interface_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
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

    /// Parses one eval interface constant, property contract, or method signature.
    pub(super) fn parse_interface_member(
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
            constants.push(
                self.parse_class_const_decl(EvalVisibility::Public, is_final)?
                    .with_attributes(attributes),
            );
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
    pub(super) fn parse_interface_method_decl_after_function_keyword(
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
    pub(super) fn parse_interface_property_decl(
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
    pub(super) fn parse_property_hook_contracts(&mut self) -> Result<(bool, bool), EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut requires_get = false;
        let mut requires_set = false;
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            if self.consume(TokenKind::Ampersand) {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            let TokenKind::Ident(hook_name) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let is_get = ident_eq(hook_name, "get");
            let is_set = ident_eq(hook_name, "set");
            if !is_get && !is_set {
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
    pub(super) fn parse_interface_property_hook_contracts(
        &mut self,
    ) -> Result<(bool, bool), EvalParseError> {
        self.parse_property_hook_contracts()
    }

    /// Parses retained property type metadata before the `$property` token.
    pub(super) fn parse_optional_property_type(
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

    /// Parses `function name($param, ...) { ... }` declarations.
    pub(super) fn parse_function_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.parse_function_decl_stmt_with_attributes(Vec::new())
    }

    /// Parses `function name($param, ...) { ... }` declarations with attributes.
    pub(super) fn parse_function_decl_stmt_with_attributes(
        &mut self,
        attributes: Vec<EvalAttribute>,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let source_start_line = self.current_line();
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
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
        let return_type = self.parse_optional_return_type(EvalTypePosition::FunctionReturn)?;
        let (body, source_end_line) = self.parse_block_with_end_line()?;
        Ok(vec![EvalStmt::FunctionDecl {
            name,
            source_location: Some(EvalSourceLocation::new(source_start_line, source_end_line)),
            attributes,
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type,
            body,
        }])
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

    /// Parses a method parameter list and records metadata plus promotion side effects.
    pub(super) fn parse_method_params(
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
    fn parse_promoted_parameter_modifiers(
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
    fn parse_optional_promoted_property_type(
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
    fn parse_optional_parameter_type(
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
    fn parse_optional_return_type(
        &mut self,
        position: EvalTypePosition,
    ) -> Result<Option<EvalParameterType>, EvalParseError> {
        if !self.consume(TokenKind::Colon) {
            return Ok(None);
        }
        self.parse_type_decl(position)
    }

    /// Parses one PHP type declaration and returns retained eval metadata.
    fn parse_type_decl(
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
    fn next_token_starts_parameter_storage(&self) -> bool {
        matches!(self.peek(), TokenKind::DollarIdent(_) | TokenKind::Ellipsis)
    }

    /// Consumes one simple qualified method type name.
    fn parse_type_name(
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
    fn type_variant_from_name(
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

    /// Parses `unset($name[, ...]);` with variable, array-access, and property operands.
    pub(super) fn parse_unset_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let mut statements = Vec::new();
        loop {
            let target = self.parse_expr()?;
            let stmt = match target {
                EvalExpr::ArrayGet { array, index } => EvalStmt::UnsetArrayElement {
                    array: *array,
                    index: *index,
                },
                EvalExpr::LoadVar(name) => EvalStmt::UnsetVar { name },
                EvalExpr::PropertyGet { object, property } => EvalStmt::UnsetProperty {
                    object: *object,
                    property,
                },
                _ => return Err(EvalParseError::ExpectedVariable),
            };
            statements.push(stmt);
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

    /// Parses a brace-delimited statement block and returns the closing-brace line.
    pub(super) fn parse_block_with_end_line(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, i64), EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        self.parse_nested_block_contents_with_end_line()
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

    /// Parses a nested block and returns the closing-brace line.
    pub(super) fn parse_nested_block_contents_with_end_line(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, i64), EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_block_contents_with_end_line();
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

    /// Parses statements until the closing brace and returns that brace's line.
    pub(super) fn parse_block_contents_with_end_line(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, i64), EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            statements.extend(self.parse_stmt()?);
        }
        let source_end_line = self.current_line();
        self.expect(TokenKind::RBrace)?;
        Ok((statements, source_end_line))
    }
}

/// Returns whether an eval method parameter default can be materialized safely.
fn method_parameter_default_is_supported(default: &EvalExpr) -> bool {
    eval_constant_expression_default_is_supported(default)
}

/// Returns whether an EvalIR expression is safe to retain as a method default.
fn eval_constant_expression_default_is_supported(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Array(elements) => elements.iter().all(eval_array_element_default_is_supported),
        EvalExpr::Const(_) => true,
        EvalExpr::Magic(_) => true,
        EvalExpr::ConstFetch(_) | EvalExpr::NamespacedConstFetch { .. } => true,
        EvalExpr::ClassConstantFetch { class_name, .. }
        | EvalExpr::ClassNameFetch { class_name } => {
            eval_default_class_receiver_is_supported(class_name)
        }
        EvalExpr::NewObject { class_name, args } => {
            eval_default_class_receiver_is_supported(class_name)
                && args.iter().all(eval_call_arg_default_is_supported)
        }
        EvalExpr::NewAnonymousClass { .. } => false,
        EvalExpr::NullCoalesce { value, default } => {
            eval_constant_expression_default_is_supported(value)
                && eval_constant_expression_default_is_supported(default)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_constant_expression_default_is_supported(condition)
                && then_branch
                    .as_deref()
                    .is_none_or(eval_constant_expression_default_is_supported)
                && eval_constant_expression_default_is_supported(else_branch)
        }
        EvalExpr::Cast { expr, .. } => eval_constant_expression_default_is_supported(expr),
        EvalExpr::Unary { expr, .. } => eval_constant_expression_default_is_supported(expr),
        EvalExpr::Binary { left, right, .. } => {
            eval_constant_expression_default_is_supported(left)
                && eval_constant_expression_default_is_supported(right)
        }
        _ => false,
    }
}

/// Returns whether one object-construction argument is safe inside a method default.
fn eval_call_arg_default_is_supported(arg: &EvalCallArg) -> bool {
    !arg.is_spread() && eval_constant_expression_default_is_supported(arg.value())
}

/// Returns whether one array default element contains only supported constant expressions.
fn eval_array_element_default_is_supported(element: &EvalArrayElement) -> bool {
    match element {
        EvalArrayElement::Value(value) => eval_constant_expression_default_is_supported(value),
        EvalArrayElement::KeyValue { key, value } => {
            eval_constant_expression_default_is_supported(key)
                && eval_constant_expression_default_is_supported(value)
        }
    }
}

/// Returns whether a type list contains return-only standalone atoms.
fn type_variants_contain_standalone_return_only_atoms(
    variants: &[EvalParameterTypeVariant],
) -> bool {
    variants.iter().any(|variant| {
        matches!(
            variant,
            EvalParameterTypeVariant::Never | EvalParameterTypeVariant::Void
        )
    })
}

/// Returns whether the type position accepts standalone return-only atoms.
fn type_position_allows_return_only_atoms(position: EvalTypePosition) -> bool {
    matches!(
        position,
        EvalTypePosition::FunctionReturn | EvalTypePosition::MethodReturn
    )
}

/// Returns whether `self` and `parent` are legal in this type position.
fn type_position_allows_class_scope_atoms(position: EvalTypePosition) -> bool {
    !matches!(
        position,
        EvalTypePosition::FunctionParameter | EvalTypePosition::FunctionReturn
    )
}

/// Returns whether a class-like receiver is legal in a compile-time method default.
fn eval_default_class_receiver_is_supported(class_name: &str) -> bool {
    !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("static")
}

/// Converts a parsed attribute argument expression into retained literal metadata.
fn eval_attribute_arg_from_expr(expr: &EvalExpr) -> Option<EvalAttributeArg> {
    match expr {
        EvalExpr::Const(EvalConst::String(value)) => Some(EvalAttributeArg::String(value.clone())),
        EvalExpr::Const(EvalConst::Int(value)) => Some(EvalAttributeArg::Int(*value)),
        EvalExpr::Const(EvalConst::Float(value)) => Some(EvalAttributeArg::Float(value.to_bits())),
        EvalExpr::Const(EvalConst::Bool(value)) => Some(EvalAttributeArg::Bool(*value)),
        EvalExpr::Const(EvalConst::Null) => Some(EvalAttributeArg::Null),
        EvalExpr::Unary {
            op: EvalUnaryOp::Negate,
            expr,
        } => match expr.as_ref() {
            EvalExpr::Const(EvalConst::Int(value)) => {
                Some(EvalAttributeArg::Int(value.wrapping_neg()))
            }
            EvalExpr::Const(EvalConst::Float(value)) => {
                Some(EvalAttributeArg::Float((-*value).to_bits()))
            }
            _ => None,
        },
        EvalExpr::ClassNameFetch { class_name } => {
            eval_attribute_class_name_arg(class_name).map(EvalAttributeArg::String)
        }
        EvalExpr::Array(elements) => eval_attribute_array_arg_from_elements(elements),
        _ => None,
    }
}

/// Converts a positional eval array literal into retained attribute metadata.
fn eval_attribute_array_arg_from_elements(
    elements: &[EvalArrayElement],
) -> Option<EvalAttributeArg> {
    elements
        .iter()
        .map(|element| match element {
            EvalArrayElement::Value(value) => eval_attribute_arg_from_expr(value),
            EvalArrayElement::KeyValue { .. } => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(EvalAttributeArg::Array)
}

/// Returns a compile-time class-name string for named `ClassName::class` attribute args.
fn eval_attribute_class_name_arg(class_name: &str) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    if ["self", "parent", "static"]
        .iter()
        .any(|special| class_name.eq_ignore_ascii_case(special))
    {
        return None;
    }
    Some(class_name.to_string())
}

/// Returns whether any parsed property hook accessor uses its own backing slot.
fn property_hook_methods_use_backing_slot(
    hook_methods: &[EvalClassMethod],
    property_name: &str,
) -> bool {
    hook_methods.iter().any(|method| {
        method
            .body()
            .iter()
            .any(|stmt| eval_stmt_uses_this_property(stmt, property_name))
    })
}

/// Returns whether one statement touches `$this->{$property_name}` directly.
fn eval_stmt_uses_this_property(stmt: &EvalStmt, property_name: &str) -> bool {
    match stmt {
        EvalStmt::ArrayAppendVar { value, .. } => {
            eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::ArraySetVar { index, value, .. } => {
            eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::Break
        | EvalStmt::Continue
        | EvalStmt::ClassDecl(_)
        | EvalStmt::EnumDecl(_)
        | EvalStmt::FunctionDecl { .. }
        | EvalStmt::Global { .. }
        | EvalStmt::InterfaceDecl(_)
        | EvalStmt::ReferenceAssign { .. }
        | EvalStmt::TraitDecl(_)
        | EvalStmt::UnsetVar { .. } => false,
        EvalStmt::UnsetArrayElement { array, index } => {
            eval_expr_uses_this_property(array, property_name)
                || eval_expr_uses_this_property(index, property_name)
        }
        EvalStmt::DoWhile { body, condition } | EvalStmt::While { condition, body } => {
            eval_expr_uses_this_property(condition, property_name)
                || eval_stmt_list_uses_this_property(body, property_name)
        }
        EvalStmt::Echo(expr)
        | EvalStmt::Expr(expr)
        | EvalStmt::StaticVar { init: expr, .. }
        | EvalStmt::Throw(expr) => eval_expr_uses_this_property(expr, property_name),
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => {
            eval_stmt_list_uses_this_property(init, property_name)
                || condition
                    .as_ref()
                    .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
                || eval_stmt_list_uses_this_property(update, property_name)
                || eval_stmt_list_uses_this_property(body, property_name)
        }
        EvalStmt::Foreach { array, body, .. } => {
            eval_expr_uses_this_property(array, property_name)
                || eval_stmt_list_uses_this_property(body, property_name)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_expr_uses_this_property(condition, property_name)
                || eval_stmt_list_uses_this_property(then_branch, property_name)
                || eval_stmt_list_uses_this_property(else_branch, property_name)
        }
        EvalStmt::Return(expr) => expr
            .as_ref()
            .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name)),
        EvalStmt::PropertyReferenceBind {
            object, property, ..
        }
        | EvalStmt::UnsetProperty { object, property } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
        }
        EvalStmt::PropertySet {
            object,
            property,
            value,
        } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::StaticPropertySet { value, .. } | EvalStmt::StoreVar { value, .. } => {
            eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::Switch { expr, cases } => {
            eval_expr_uses_this_property(expr, property_name)
                || cases.iter().any(|case| {
                    case.condition
                        .as_ref()
                        .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
                        || eval_stmt_list_uses_this_property(&case.body, property_name)
                })
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => {
            eval_stmt_list_uses_this_property(body, property_name)
                || catches
                    .iter()
                    .any(|catch| eval_stmt_list_uses_this_property(&catch.body, property_name))
                || eval_stmt_list_uses_this_property(finally_body, property_name)
        }
    }
}

/// Returns whether any statement in a list touches `$this->{$property_name}` directly.
fn eval_stmt_list_uses_this_property(stmts: &[EvalStmt], property_name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| eval_stmt_uses_this_property(stmt, property_name))
}

/// Returns whether one expression touches `$this->{$property_name}` directly.
fn eval_expr_uses_this_property(expr: &EvalExpr, property_name: &str) -> bool {
    match expr {
        EvalExpr::Array(elements) => elements.iter().any(|element| match element {
            EvalArrayElement::Value(value) => eval_expr_uses_this_property(value, property_name),
            EvalArrayElement::KeyValue { key, value } => {
                eval_expr_uses_this_property(key, property_name)
                    || eval_expr_uses_this_property(value, property_name)
            }
        }),
        EvalExpr::ArrayGet { array, index } => {
            eval_expr_uses_this_property(array, property_name)
                || eval_expr_uses_this_property(index, property_name)
        }
        EvalExpr::Call { args, .. }
        | EvalExpr::NamespacedCall { args, .. }
        | EvalExpr::NewObject { args, .. }
        | EvalExpr::StaticMethodCall { args, .. } => args
            .iter()
            .any(|arg| eval_expr_uses_this_property(arg.value(), property_name)),
        EvalExpr::DynamicCall { callee, args } => {
            eval_expr_uses_this_property(callee, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::Const(_)
        | EvalExpr::ConstFetch(_)
        | EvalExpr::ClassConstantFetch { .. }
        | EvalExpr::ClassNameFetch { .. }
        | EvalExpr::LoadVar(_)
        | EvalExpr::Magic(_)
        | EvalExpr::NamespacedConstFetch { .. }
        | EvalExpr::StaticPropertyGet { .. } => false,
        EvalExpr::Include { path, .. }
        | EvalExpr::Cast { expr: path, .. }
        | EvalExpr::Clone(path)
        | EvalExpr::Print(path)
        | EvalExpr::Unary { expr: path, .. } => eval_expr_uses_this_property(path, property_name),
        EvalExpr::InstanceOf { value, target } => {
            eval_expr_uses_this_property(value, property_name)
                || matches!(
                    target,
                    EvalInstanceOfTarget::Expr(target)
                        if eval_expr_uses_this_property(target, property_name)
                )
        }
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => {
            eval_expr_uses_this_property(subject, property_name)
                || arms.iter().any(|arm| {
                    arm.patterns
                        .iter()
                        .any(|pattern| eval_expr_uses_this_property(pattern, property_name))
                        || eval_expr_uses_this_property(&arm.value, property_name)
                })
                || default
                    .as_ref()
                    .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
        }
        EvalExpr::MethodCall { object, args, .. } => {
            eval_expr_uses_this_property(object, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::NewAnonymousClass { args, .. } => args
            .iter()
            .any(|arg| eval_expr_uses_this_property(arg.value(), property_name)),
        EvalExpr::NullCoalesce { value, default } => {
            eval_expr_uses_this_property(value, property_name)
                || eval_expr_uses_this_property(default, property_name)
        }
        EvalExpr::PropertyGet { object, property } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_expr_uses_this_property(condition, property_name)
                || then_branch
                    .as_ref()
                    .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
                || eval_expr_uses_this_property(else_branch, property_name)
        }
        EvalExpr::Binary { left, right, .. } => {
            eval_expr_uses_this_property(left, property_name)
                || eval_expr_uses_this_property(right, property_name)
        }
    }
}

/// Returns whether one object/property pair is exactly `$this->{$property_name}`.
fn eval_is_this_property(object: &EvalExpr, property: &str, property_name: &str) -> bool {
    matches!(object, EvalExpr::LoadVar(name) if name == "this") && property == property_name
}

/// Returns the synthetic get-hook method name for one property.
fn property_hook_get_method(property_name: &str) -> String {
    format!("__propget_{property_name}")
}

/// Returns the synthetic set-hook method name for one property.
fn property_hook_set_method(property_name: &str) -> String {
    format!("__propset_{property_name}")
}

/// Builds the implicit constructor assignment or alias for a promoted parameter.
fn promoted_property_assignment(name: &str, is_by_ref: bool) -> EvalStmt {
    if is_by_ref {
        EvalStmt::PropertyReferenceBind {
            object: EvalExpr::LoadVar("this".to_string()),
            property: name.to_string(),
            source: name.to_string(),
        }
    } else {
        EvalStmt::PropertySet {
            object: EvalExpr::LoadVar("this".to_string()),
            property: name.to_string(),
            value: EvalExpr::LoadVar(name.to_string()),
        }
    }
}
