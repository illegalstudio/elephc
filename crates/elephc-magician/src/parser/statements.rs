//! Purpose:
//! Dispatches PHP eval statements and owns shared parser statement metadata.
//! Syntax families and analysis helpers live in focused child modules.
//!
//! Called from:
//! - `crate::parser::state::Parser::parse_program()`.
//!
//! Key details:
//! - Statement parsing expands multi-variable constructs such as `unset($a, $b)` into multiple EvalIR statements.
//! - Namespace/use parsing lives here because declarations are statement-level syntax in PHP.

mod assignments;
mod class_declarations;
mod class_members;
mod control_flow;
mod default_validation;
mod enums;
mod functions_namespaces;
mod globals_exceptions;
mod interfaces;
mod loops;
mod parameters_types;
mod property_builders;
mod property_hook_analysis;
mod traits;

use super::cursor::*;
use super::state::*;
use crate::errors::EvalParseError;
use crate::eval_ir::{
    EvalArrayElement, EvalAttribute, EvalAttributeArg, EvalBinOp, EvalCallArg, EvalCatch, EvalClass,
    EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalConst, EvalEnum,
    EvalEnumBackingType, EvalEnumCase, EvalExpr, EvalInstanceOfTarget, EvalInterface,
    EvalInterfaceMethod, EvalInterfaceProperty, EvalParameterType, EvalParameterTypeVariant,
    EvalSourceLocation, EvalStmt, EvalSwitchCase, EvalTrait, EvalTraitAdaptation, EvalUnaryOp,
    EvalVisibility,
};
use crate::lexer::TokenKind;

use default_validation::*;
use property_builders::*;
use property_hook_analysis::*;

/// Parsed method parameters plus constructor-promotion side products.
pub(super) struct ParsedMethodParams {
    pub(super) params: Vec<String>,
    pub(super) parameter_attributes: Vec<Vec<EvalAttribute>>,
    pub(super) parameter_types: Vec<Option<EvalParameterType>>,
    pub(super) parameter_defaults: Vec<Option<EvalExpr>>,
    pub(super) parameter_is_by_ref: Vec<bool>,
    pub(super) parameter_is_variadic: Vec<bool>,
    pub(super) promoted_properties: Vec<EvalClassProperty>,
    pub(super) promoted_assignments: Vec<EvalStmt>,
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
pub(super) enum EvalTypePosition {
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
            TokenKind::Ident(_) | TokenKind::Backslash
                if self.current_starts_static_property_postfix_inc_dec() =>
            {
                self.parse_static_property_inc_dec_stmt(false, true)
            }
            TokenKind::DollarIdent(_) if self.current_starts_dynamic_static_property_assignment() => {
                self.parse_dynamic_static_property_set_stmt(true)
            }
            TokenKind::DollarIdent(_)
                if self.current_starts_dynamic_static_property_postfix_inc_dec() =>
            {
                self.parse_dynamic_static_property_inc_dec_stmt(false, true)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus
                if self.current_starts_prefixed_static_property_inc_dec() =>
            {
                self.parse_static_property_inc_dec_stmt(true, true)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus
                if self.current_starts_prefixed_dynamic_static_property_inc_dec() =>
            {
                self.parse_dynamic_static_property_inc_dec_stmt(true, true)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus
                if self.current_starts_prefixed_property_inc_dec() =>
            {
                self.parse_prefixed_property_inc_dec_stmt(true)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus => {
                self.parse_prefix_inc_dec_stmt(true)
            }
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
                self.parse_property_like_stmt_tail(expr, true)
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

    /// Returns true when current tokens form direct or indexed `Class::$property` assignment.
    pub(super) fn current_starts_static_property_assignment(&self) -> bool {
        self.static_property_tokens_end(self.pos)
            .is_some_and(|pos| self.static_property_assignment_suffix_starts(pos))
    }

    /// Returns true when current tokens form direct or indexed `$class::$property` assignment.
    pub(super) fn current_starts_dynamic_static_property_assignment(&self) -> bool {
        matches!(self.current(), TokenKind::DollarIdent(_))
            && matches!(self.peek(), TokenKind::DoubleColon)
            && matches!(self.tokens.get(self.pos + 2), Some(TokenKind::DollarIdent(_)))
            && self.static_property_assignment_suffix_starts(self.pos + 3)
    }

    /// Returns true when the current tokens form `Class::$property++` or `Class::$property--`.
    pub(super) fn current_starts_static_property_postfix_inc_dec(&self) -> bool {
        self.static_property_tokens_end(self.pos).is_some_and(|pos| {
            matches!(
                self.tokens.get(pos),
                Some(TokenKind::PlusPlus | TokenKind::MinusMinus)
            )
        })
    }

    /// Returns true when the current tokens form `$class::$property++` or `$class::$property--`.
    pub(super) fn current_starts_dynamic_static_property_postfix_inc_dec(&self) -> bool {
        matches!(self.current(), TokenKind::DollarIdent(_))
            && matches!(self.peek(), TokenKind::DoubleColon)
            && matches!(self.tokens.get(self.pos + 2), Some(TokenKind::DollarIdent(_)))
            && matches!(
                self.tokens.get(self.pos + 3),
                Some(TokenKind::PlusPlus | TokenKind::MinusMinus)
            )
    }

    /// Returns true when the current tokens form `++Class::$property` or `--Class::$property`.
    pub(super) fn current_starts_prefixed_static_property_inc_dec(&self) -> bool {
        matches!(self.current(), TokenKind::PlusPlus | TokenKind::MinusMinus)
            && self.static_property_tokens_end(self.pos + 1).is_some()
    }

    /// Returns true when the current tokens form `++$class::$property` or `--$class::$property`.
    pub(super) fn current_starts_prefixed_dynamic_static_property_inc_dec(&self) -> bool {
        matches!(self.current(), TokenKind::PlusPlus | TokenKind::MinusMinus)
            && matches!(self.tokens.get(self.pos + 1), Some(TokenKind::DollarIdent(_)))
            && matches!(self.tokens.get(self.pos + 2), Some(TokenKind::DoubleColon))
            && matches!(self.tokens.get(self.pos + 3), Some(TokenKind::DollarIdent(_)))
    }

    /// Returns true when the current tokens form `++$object->property` or `--$object->property`.
    pub(super) fn current_starts_prefixed_property_inc_dec(&self) -> bool {
        matches!(self.current(), TokenKind::PlusPlus | TokenKind::MinusMinus)
            && matches!(self.tokens.get(self.pos + 1), Some(TokenKind::DollarIdent(_)))
            && matches!(self.tokens.get(self.pos + 2), Some(TokenKind::Arrow))
    }

    /// Returns the token position after `Class::$property` when present at `pos`.
    fn static_property_tokens_end(&self, mut pos: usize) -> Option<usize> {
        if matches!(self.tokens.get(pos), Some(TokenKind::Backslash)) {
            pos += 1;
        }
        if !matches!(self.tokens.get(pos), Some(TokenKind::Ident(_))) {
            return None;
        }
        pos += 1;
        while matches!(self.tokens.get(pos), Some(TokenKind::Backslash)) {
            pos += 1;
            if !matches!(self.tokens.get(pos), Some(TokenKind::Ident(_))) {
                return None;
            }
            pos += 1;
        }
        if !matches!(self.tokens.get(pos), Some(TokenKind::DoubleColon)) {
            return None;
        }
        pos += 1;
        if !matches!(self.tokens.get(pos), Some(TokenKind::DollarIdent(_))) {
            return None;
        }
        Some(pos + 1)
    }

    /// Returns true when tokens after a static property form direct or indexed assignment.
    fn static_property_assignment_suffix_starts(&self, pos: usize) -> bool {
        self.tokens
            .get(pos)
            .is_some_and(|token| assignment_op(token).is_some())
            || self.static_property_array_assignment_suffix_starts(pos)
    }

    /// Returns true when tokens at `pos` form `[expr] <assign-op>` or `[] =`.
    fn static_property_array_assignment_suffix_starts(&self, pos: usize) -> bool {
        if !matches!(self.tokens.get(pos), Some(TokenKind::LBracket)) {
            return false;
        }
        let mut cursor = pos;
        let mut depth = 0usize;
        loop {
            match self.tokens.get(cursor) {
                Some(TokenKind::LBracket) => depth += 1,
                Some(TokenKind::RBracket) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self
                            .tokens
                            .get(cursor + 1)
                            .is_some_and(|token| assignment_op(token).is_some());
                    }
                }
                Some(TokenKind::Eof) | None => return false,
                _ => {}
            }
            cursor += 1;
        }
    }
}
