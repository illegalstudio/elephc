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
    if !profile.contains(StrictFeature::DynamicCode) && !profile.contains(StrictFeature::Bools) { return Ok(()); }
    crate::compiler_stack::with_compiler_stack(|| {
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
        let ExprKind::FunctionCall { name, args } = &expr.kind else { return; };
        if php_symbol_key(name.as_str().trim_start_matches('\\')) != "eval" { return; }
        let [Expr { kind: ExprKind::StringLiteral(source), .. }] = args.as_slice() else {
            if !self.profile.contains(StrictFeature::DynamicCode) { return; }
            self.error = Some(CompileError::new(expr.span,
                "--strict-dynamic-code requires literal eval source verified during compilation"));
            return;
        };
        let verification_flag = if self.profile.contains(StrictFeature::DynamicCode) { "strict-dynamic-code" } else { "strict-bools" };
        if self.depth >= 64 {
            self.error = Some(CompileError::new(expr.span,
                &format!("--{verification_flag} literal eval nesting exceeds 64 levels")));
            return;
        }
        let parsed = crate::lexer::tokenize(&format!("<?php {source}"))
            .and_then(|tokens| crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php));
        match parsed {
            Ok(program) => {
                self.depth += 1;
                walk_program(&program, self);
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
            ExprKind::Negate(_) | ExprKind::BitNot(_) => true,
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
