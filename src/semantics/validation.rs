//! Purpose:
//! Checks source restrictions before optimization can erase their original syntax.
//!
//! Called from:
//! - The compilation pipeline and the public checker entry point.
//!
//! Key details:
//! - Literal eval is recursively audited; native eligibility is checked after lowering.

use super::array_keys::{validate_source_writes, SourceWriteScope};
use super::{SemanticProfile, StrictFeature};
use crate::ast_visit::{walk_program, AstVisitor};
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ArrayEntry, BinOp, CastType, Expr, ExprKind, Program, Stmt, StmtKind};

/// Guarantees requiring original source checks and recursive literal-eval validation.
const SOURCE_GUARANTEES: &[StrictFeature] = &[
    StrictFeature::DynamicCode, StrictFeature::Bools, StrictFeature::Overflow,
    StrictFeature::Arithmetic, StrictFeature::Coercions, StrictFeature::Casts,
    StrictFeature::Comparisons, StrictFeature::Strings, StrictFeature::ArrayKeys, StrictFeature::Nullability,
];

/// Validates syntax-dependent guarantees without pruning any branch or declaration.
pub fn validate_source(program: &Program, profile: SemanticProfile) -> Result<(), CompileError> {
    if !SOURCE_GUARANTEES.iter().copied().any(|feature| profile.contains(feature)) { return Ok(()); }
    crate::compiler_stack::with_compiler_stack(|| {
        super::numeric_constants::validate(program, profile)?;
        if profile.contains(StrictFeature::ArrayKeys) {
            validate_source_writes(program, profile, SourceWriteScope::Program)?;
        }
        let mut validator = SourceValidator::new(profile);
        walk_program(program, &mut validator);
        validator.error.map_or(Ok(()), Err)
    })
}

/// Checks a declaration's resolved constant operands before binding can fold them away.
pub(crate) fn validate_bound_expression(expr: Expr, profile: SemanticProfile) -> Result<(), CompileError> {
    if !SOURCE_GUARANTEES.iter().copied().any(|feature| profile.contains(feature)) { return Ok(()); }
    let source = expr.source_profile;
    let span = expr.span;
    let mut statement = Stmt::new(StmtKind::ExprStmt(expr), span);
    statement.source_mode = source.mode;
    statement.strict_types = source.strict_types;
    let mut validator = SourceValidator::new(profile);
    crate::ast_visit::walk_statements(std::slice::from_ref(&statement), &mut validator);
    validator.error.map_or(Ok(()), Err)
}

/// Tracks nested literal source independently of compiler-global configuration.
struct SourceValidator {
    depth: usize,
    profile: SemanticProfile,
    source_modes: crate::ast_visit::SourcePolicy,
    error: Option<CompileError>,
    quiet_operations: Vec<crate::span::Span>,
    quiet_frames: Vec<usize>,
    /// Inherits the physical call-site file through recursively parsed eval fragments.
    eval_source_paths: Vec<Option<String>>,
}

impl SourceValidator {
    /// Starts validation with no enclosing source or quiet-access state.
    fn new(profile: SemanticProfile) -> Self {
        Self { depth: 0, error: None, profile,
            source_modes: Default::default(), quiet_operations: Vec::new(), quiet_frames: Vec::new(),
            eval_source_paths: Vec::new() }
    }
}

impl AstVisitor for SourceValidator {
    /// Tracks physical source for trusted implementation statements and validates raw conditions.
    fn statement(&mut self, stmt: &Stmt) {
        self.source_modes.enter_statement(stmt);
        self.quiet_frames.push(self.quiet_operations.len());
        if let StmtKind::NestedArrayAssign { target, .. } = &stmt.kind {
            super::nullability::quiet_spine(target, &mut self.quiet_operations);
        }
        if let StmtKind::Echo(value) = &stmt.kind {
            if let Some(ty) = literal_type(value) {
                if let Err(error) = super::strings::validate_formatting(self.profile, stmt.source_mode, &ty, value.span) {
                    self.error = Some(error);
                }
            }
        }
        if super::comparisons::active(self.profile, stmt.source_mode) {
            if let StmtKind::Switch { subject, cases, .. } = &stmt.kind {
                if let Some(subject) = literal_type(subject) {
                    for (labels, _) in cases {
                        for label in labels {
                            if let Some(case) = literal_type(label) {
                                if super::numeric::value_kind(&subject) != super::numeric::value_kind(&case) {
                                    self.error = Some(CompileError::new(label.span,
                                        "--strict-comparisons: switch label cannot match the subject's value kind"));
                                }
                            }
                        }
                    }
                }
            }
        }
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
        self.quiet_operations.truncate(self.quiet_frames.pop().expect("balanced quiet statement"));
        self.source_modes.leave_statement();
    }

    /// Requires eval source to be literal and recursively verifies its source restrictions.
    fn expression(&mut self, expr: &Expr) {
        let source_mode = self.source_modes.enter_expression(expr).mode;
        self.quiet_frames.push(self.quiet_operations.len());
        super::nullability::expression_quiet_spines(expr, &mut self.quiet_operations);
        if self.error.is_some() { return; }
        let receiver = match &expr.kind {
            ExprKind::ArrayAccess { array, .. } => Some(array),
            ExprKind::PropertyAccess { object, .. } | ExprKind::DynamicPropertyAccess { object, .. }
            | ExprKind::MethodCall { object, .. } => Some(object),
            _ => None,
        };
        if let Some(ty) = receiver.and_then(|receiver| literal_type(receiver)) {
            if let Err(error) = super::nullability::validate_receiver(self.profile, source_mode,
                &ty, self.quiet_operations.contains(&expr.span), expr.span) {
                self.error = Some(error);
                return;
            }
        }
        if self.profile.contains(StrictFeature::ArrayKeys) {
            match &expr.kind {
                ExprKind::ArrayLiteralAssoc(entries) => {
                    for (key, _) in entries { self.array_key(key); }
                }
                ExprKind::ArrayLiteralMixed(entries) => {
                    for entry in entries {
                        if let ArrayEntry::Keyed(key, _) = entry { self.array_key(key); }
                    }
                }
                ExprKind::ArrayAccess { array, index } if matches!(array.kind,
                    ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
                    | ExprKind::ArrayLiteralMixed(_) | ExprKind::Cast { target: CastType::Array, .. }) => self.array_key(index),
                _ => {}
            }
            if self.error.is_some() { return; }
        }
        if super::strings::active(self.profile, source_mode) {
            let result = match &expr.kind {
                ExprKind::BinaryOp { left, op: BinOp::Concat, right } => [left, right].into_iter()
                    .try_for_each(|operand| concat_operand_type(operand).map_or(Ok(()), |ty|
                        super::strings::validate_concatenation(self.profile, source_mode, &ty, operand.span))),
                ExprKind::Print(inner) | ExprKind::Cast { target: CastType::StringFormat, expr: inner } =>
                    literal_type(inner).map_or(Ok(()), |ty|
                        super::strings::validate_formatting(self.profile, source_mode, &ty, inner.span)),
                _ => Ok(()),
            };
            if let Err(error) = result { self.error = Some(error); return; }
        }
        if super::comparisons::active(self.profile, source_mode)
        {
            if let ExprKind::BinaryOp { left, op, right } = &expr.kind {
                if matches!(op, BinOp::Eq | BinOp::NotEq) {
                    self.error = Some(CompileError::new(expr.span,
                        elephc_semantics::comparisons::EQUALITY_MESSAGE));
                    return;
                }
                if let (Some(left), Some(right)) = (literal_type(left), literal_type(right)) {
                    if let Err(error) = super::comparisons::validate(self.profile,
                        source_mode, op, &left, &right, expr.span)
                    { self.error = Some(error); return; }
                }
            }
        }
        if self.profile.contains(StrictFeature::Bools) && source_mode != crate::source::SourceMode::Internal {
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
        if source_mode != crate::source::SourceMode::Internal {
            let unary = match &expr.kind {
                ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner) => Some((inner, elephc_semantics::numeric_domains::NumericOperator::Unary)),
                ExprKind::BitNot(inner) => Some((inner, elephc_semantics::numeric_domains::NumericOperator::BitNot)),
                _ => None,
            };
            if let Some((inner, operator)) = unary {
                if let (Some(rule), Some(ty)) = (super::numeric::operator_rule(self.profile, source_mode, operator), literal_type(inner)) {
                    if let Err(error) = super::numeric::validate_unary_type(rule, &ty, expr.span) {
                        self.error = Some(error); return;
                    }
                }
            }
            if let ExprKind::BinaryOp { left, op, right } = &expr.kind {
                if let Some(rule) = super::numeric::operand_rule(self.profile, source_mode, op) {
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
        let verification_flag = SOURCE_GUARANTEES.iter().copied()
            .find(|feature| self.profile.contains(*feature)).expect("source verification was requested").flag();
        if self.depth >= 64 {
            self.error = Some(CompileError::new(expr.span,
                &format!("--{verification_flag} literal eval nesting exceeds 64 levels")));
            return;
        }
        let source_id = crate::span::Span::fresh_synthetic_source_id();
        let source_path = expr.span.source_path().or_else(|| {
            self.eval_source_paths
                .last()
                .and_then(|source_path| source_path.clone())
        });
        let parsed = crate::lexer::tokenize_with_semantics(&format!("<?php {source}"),
            crate::source::SourceMode::Php, source_id, self.profile)
            .and_then(|tokens| crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php));
        match parsed {
            Ok(program) => {
                let original_declarations =
                    super::nullability::validate_original_declarations(&program, self.profile);
                let program = crate::magic_constants::substitute_eval_constants(
                    program,
                    source_path.as_deref().map(std::path::Path::new),
                );
                self.depth += 1;
                match original_declarations
                    .and_then(|()| super::numeric_constants::validate(&program, self.profile))
                    .and_then(|()| {
                        if self.profile.contains(StrictFeature::ArrayKeys) {
                            validate_source_writes(
                                &program,
                                self.profile,
                                SourceWriteScope::LiteralEval,
                            )
                        } else {
                            Ok(())
                        }
                    })
                {
                    Ok(()) => {
                        self.eval_source_paths.push(source_path);
                        walk_program(&program, self);
                        self.eval_source_paths.pop();
                    }
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

    /// Restores the enclosing producer after recursive operands and literal eval have been visited.
    fn expression_end(&mut self, _expr: &Expr) {
        self.quiet_operations.truncate(self.quiet_frames.pop().expect("balanced quiet expression"));
        self.source_modes.leave_expression();
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
            CastType::Bool => PhpType::Bool, CastType::String | CastType::StringFormat => PhpType::Str,
            CastType::Array => PhpType::Array(Box::new(PhpType::Never)), _ => return None,
        },
        _ => return None,
    })
}

/// Includes obvious object operands without changing numeric native-class source proofs.
fn concat_operand_type(expr: &Expr) -> Option<crate::types::PhpType> {
    literal_type(expr).or_else(|| match &expr.kind {
        ExprKind::NewObject { .. } | ExprKind::NewGeneric { .. } | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. } | ExprKind::NewScopedObject { .. } =>
            Some(crate::types::PhpType::Object("object".into())),
        _ => None,
    })
}

impl SourceValidator {
    /// Keeps original key kinds visible before folding, including boolean-producing syntax.
    fn array_key(&mut self, expr: &Expr) {
        if self.error.is_some() { return; }
        let ty = concat_operand_type(expr).or_else(|| match &expr.kind {
            ExprKind::Not(_) | ExprKind::BinaryOp { op: BinOp::Eq | BinOp::NotEq
                | BinOp::StrictEq | BinOp::StrictNotEq | BinOp::Lt | BinOp::Gt
                | BinOp::LtEq | BinOp::GtEq | BinOp::And | BinOp::Or | BinOp::Xor, .. } =>
                Some(crate::types::PhpType::Bool),
            ExprKind::BinaryOp { op: BinOp::Div, .. } => Some(crate::types::PhpType::Float),
            ExprKind::Cast { target: CastType::Object, .. } => Some(crate::types::PhpType::Object(String::new())),
            _ => None,
        });
        if let Some(ty) = ty {
            if let Err(error) = super::array_keys::validate(self.profile, &ty, expr.span) {
                self.error = Some(error);
            }
        }
    }

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
