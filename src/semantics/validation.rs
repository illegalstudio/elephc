//! Purpose:
//! Checks source restrictions before optimization can erase their original syntax.
//!
//! Called from:
//! - The compilation pipeline and the public checker entry point.
//!
//! Key details:
//! - Literal eval is recursively audited; native eligibility is checked after lowering.

use super::{SemanticProfile, StrictFeature};
use crate::ast_visit::{walk_program, AstVisitor};
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program, Stmt, StmtKind};

/// Validates syntax-dependent guarantees without pruning any branch or declaration.
pub fn validate_source(program: &Program, profile: SemanticProfile) -> Result<(), CompileError> {
    if ![StrictFeature::DynamicCode, StrictFeature::Bools, StrictFeature::Overflow, StrictFeature::Arithmetic, StrictFeature::Coercions].into_iter().any(|feature| profile.contains(feature)) { return Ok(()); }
    crate::compiler_stack::with_compiler_stack(|| {
        super::numeric_constants::validate(program, profile)?;
        let mut validator = SourceValidator { depth: 0, error: None, profile, source_modes: Vec::new() };
        walk_program(program, &mut validator);
        validator.error.map_or(Ok(()), Err)
    })
}

/// Tracks nested literal source independently of compiler-global configuration.
struct SourceValidator {
    depth: usize,
    profile: SemanticProfile,
    source_modes: Vec<crate::source::SourceMode>,
    error: Option<CompileError>,
}

impl AstVisitor for SourceValidator {
    /// Tracks physical source for trusted implementation statements and validates raw conditions.
    fn statement(&mut self, stmt: &Stmt) {
        self.source_modes.push(stmt.source_mode);
        if !self.profile.contains(StrictFeature::Bools) || stmt.source_mode == crate::source::SourceMode::Internal { return; }
        match &stmt.kind {
            StmtKind::If { condition, elseif_clauses, .. } => {
                self.boolean_literal(condition);
                for (condition, _) in elseif_clauses { self.boolean_literal(condition); }
            }
            StmtKind::While { condition, .. } | StmtKind::DoWhile { condition, .. } => self.boolean_literal(condition),
            StmtKind::For { condition: Some(condition), .. } => self.boolean_literal(condition),
            _ => {}
        }
    }

    /// Restores the enclosing statement's source after nested bodies and declarations.
    fn statement_end(&mut self, _stmt: &Stmt) {
        self.source_modes.pop();
    }

    /// Requires eval source to be literal and recursively verifies its source restrictions.
    fn expression(&mut self, expr: &Expr) {
        if self.error.is_some() { return; }
        if self.profile.contains(StrictFeature::Bools) && self.source_modes.last() != Some(&crate::source::SourceMode::Internal) {
            match &expr.kind {
                ExprKind::Not(inner) => self.boolean_literal(inner),
                ExprKind::BinaryOp { left, op: BinOp::And | BinOp::Or | BinOp::Xor, right } => {
                    self.boolean_literal(left); self.boolean_literal(right);
                }
                ExprKind::Ternary { condition, .. } => self.boolean_literal(condition),
                ExprKind::ShortTernary { value, .. } => self.boolean_literal(value),
                _ => {}
            }
        }
        if self.source_modes.last() != Some(&crate::source::SourceMode::Internal) {
            let unary = match &expr.kind {
                ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner) => Some((inner, elephc_semantics::numeric_domains::NumericOperator::Unary)),
                ExprKind::BitNot(inner) => Some((inner, elephc_semantics::numeric_domains::NumericOperator::BitNot)),
                _ => None,
            };
            if let Some((inner, operator)) = unary {
                if let (Some(rule), Some(ty)) = (super::numeric::operator_rule(self.profile, crate::source::SourceMode::Php, operator), literal_type(inner)) {
                    if let Err(error) = super::numeric::validate_unary_type(rule, &ty, expr.span) {
                        self.error = Some(error); return;
                    }
                }
            }
            if let ExprKind::BinaryOp { left, op, right } = &expr.kind {
                if let Some(rule) = super::numeric::operand_rule(self.profile, crate::source::SourceMode::Php, op) {
                    if let (Some(left), Some(right)) = (literal_type(left), literal_type(right)) {
                        let array_union = *op == BinOp::Add && matches!(left, crate::types::PhpType::Array(_))
                            && matches!(right, crate::types::PhpType::Array(_));
                        if !array_union {
                            if let Err(error) = super::numeric::validate_binary_types(rule, &left, &right, expr.span) {
                                self.error = Some(error); return;
                            }
                        }
                    }
                }
            }
        }
        let ExprKind::FunctionCall { name, args } = &expr.kind else { return; };
        if php_symbol_key(name.as_str().trim_start_matches('\\')) != "eval" { return; }
        let [Expr { kind: ExprKind::StringLiteral(source), .. }] = args.as_slice() else {
            if !self.profile.contains(StrictFeature::DynamicCode) { return; }
            self.error = Some(CompileError::new(expr.span,
                "--strict-dynamic-code requires literal eval source verified during compilation"));
            return;
        };
        let verification_flag = [StrictFeature::DynamicCode, StrictFeature::Overflow, StrictFeature::Bools,
            StrictFeature::Arithmetic, StrictFeature::Coercions].into_iter()
            .find(|feature| self.profile.contains(*feature)).expect("source verification was requested").flag();
        if self.depth >= 64 {
            self.error = Some(CompileError::new(expr.span,
                &format!("--{verification_flag} literal eval nesting exceeds 64 levels")));
            return;
        }
        let parsed = crate::lexer::tokenize_with_semantics(&format!("<?php {source}"),
            crate::source::SourceMode::Php, crate::span::Span::fresh_synthetic_source_id(), self.profile)
            .and_then(|tokens| crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php));
        match parsed {
            Ok(program) => {
                self.depth += 1;
                match super::numeric_constants::validate(&program, self.profile) {
                    Ok(()) => walk_program(&program, self),
                    Err(error) => self.error = Some(error),
                }
                self.depth -= 1;
                if let Some(error) = self.error.take() {
                    self.error = Some(CompileError::new(expr.span, &format!(
                        "{} (inside literal eval at {}:{})", error.message, error.span.line, error.span.col)));
                }
            }
            Err(error) => self.error = Some(CompileError::new(expr.span,
                &format!("--{verification_flag} cannot statically verify literal eval: {}", error.message))),
        }
    }
}

/// Preserves obvious source kinds before folding can erase a rejected value conversion.
fn literal_type(expr: &Expr) -> Option<crate::types::PhpType> {
    use crate::types::PhpType;
    Some(match &expr.kind {
        ExprKind::IntLiteral(_) => PhpType::Int, ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::BoolLiteral(_) => PhpType::Bool, ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::Null => PhpType::Void,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_) => PhpType::Array(Box::new(PhpType::Never)),
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner) => match literal_type(inner)? { value @ (PhpType::Int | PhpType::Float) => value, _ => return None },
        ExprKind::Cast { target, .. } => match target {
            CastType::Int => PhpType::Int, CastType::Float => PhpType::Float,
            CastType::Bool => PhpType::Bool, CastType::String => PhpType::Str,
            CastType::Array => PhpType::Array(Box::new(PhpType::Never)), _ => return None,
        },
        _ => return None,
    })
}

impl SourceValidator {
    /// Rejects a syntactically nonboolean condition before folding can discard it.
    fn boolean_literal(&mut self, expr: &Expr) {
        let nonboolean = match &expr.kind {
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_)
            | ExprKind::Null | ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
            | ExprKind::ArrayLiteralMixed(_) | ExprKind::NewObject { .. } => true,
            ExprKind::Cast { target, .. } => *target != CastType::Bool,
            ExprKind::BinaryOp { op, .. } => matches!(op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
                | BinOp::Concat | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight | BinOp::Spaceship),
            ExprKind::UnaryPlus(_) | ExprKind::Negate(_) | ExprKind::BitNot(_) => true,
            _ => false,
        };
        if nonboolean && self.error.is_none() {
            self.error = Some(CompileError::new(expr.span, "--strict-bools requires bool in a condition; use an explicit (bool) cast"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves rejection before folding, across nested literals and declaration defaults.
    #[test]
    fn strict_profile_dynamic_code_audits_unreachable_and_nested_source() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::DynamicCode, true);
        for source in [
            "<?php if (false) { eval($source); }",
            "<?php eval('eval($source);');",
            "<?php $closure = function () { eval($source); };",
            "<?php function source_default($value = eval($source)) {}",
        ] {
            let tokens = crate::lexer::tokenize(source).unwrap();
            let program = crate::parser::parse(&tokens).unwrap();
            assert!(validate_source(&program, profile).unwrap_err().message.contains("strict-dynamic-code"));
            assert!(validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
        }
        let program = crate::parser::parse(&crate::lexer::tokenize("<?php eval('echo 42;');").unwrap()).unwrap();
        assert!(validate_source(&program, profile).is_ok());
    }

    /// Literal verification diagnostics name the enabled guarantee rather than an unrelated flag.
    #[test]
    fn strict_profile_bools_literal_eval_verification_names_the_active_flag() {
        let tokens = crate::lexer::tokenize("<?php eval('if (');").unwrap();
        let program = crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
        let error = validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("--strict-bools"));
        assert!(!error.message.contains("--strict-dynamic-code"));
    }

}
