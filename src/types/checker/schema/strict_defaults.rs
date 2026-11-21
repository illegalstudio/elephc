//! Purpose:
//! Validates original scalar declaration values under their physical source policy.
//!
//! Called from:
//! - Checker schema orchestration after class-like constants have been resolved.
//!
//! Key details:
//! - Shared AST declaration hooks cover parameters, properties and typed constants.
//! - Ordinary validation retains ownership of unresolved or deferred default errors.

use crate::ast_visit::{walk_program, AstVisitor};
use crate::errors::CompileError;
use crate::parser::ast::{Expr, Program, Stmt, StmtKind, TypeExpr};
use crate::source::SourceMode;
use super::super::Checker;

/// Adds noncoercive value checks without selecting policy from the schema pass or a future caller.
pub(crate) fn validate_noncoercive_defaults(checker: &mut Checker, program: &Program) -> Vec<CompileError> {
    if !checker.semantic_profile.contains(elephc_semantics::StrictFeature::Coercions) { return Vec::new(); }
    let mut validator = DefaultValidator { checker, source_modes: Vec::new(), classes: Vec::new(), errors: Vec::new() };
    walk_program(program, &mut validator);
    validator.errors
}

/// Restores lexical class and source state as shared traversal enters and leaves nested declarations.
struct DefaultValidator<'a> {
    checker: &'a mut Checker,
    source_modes: Vec<SourceMode>,
    classes: Vec<Option<String>>,
    errors: Vec<CompileError>,
}

impl AstVisitor for DefaultValidator<'_> {
    /// Retains the physical declaration policy and the class that owns relative constant selectors.
    fn statement(&mut self, stmt: &Stmt) {
        self.source_modes.push(stmt.source_mode);
        self.classes.push(self.checker.current_class.clone());
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::InterfaceDecl { name, .. }
            | StmtKind::TraitDecl { name, .. } | StmtKind::EnumDecl { name, .. } => {
                self.checker.current_class = Some(name.clone());
            }
            _ => {}
        }
    }

    /// Restores state even when a nested declaration selected a different source policy.
    fn statement_end(&mut self, _stmt: &Stmt) {
        self.source_modes.pop();
        self.checker.current_class = self.classes.pop().expect("declaration scope was entered");
    }

    /// Rejects only a proven implicit scalar conversion, leaving existing deferred failures intact.
    fn declared_value(&mut self, annotation: &TypeExpr, value: &Expr) {
        let source = self.source_modes.last().copied().unwrap_or(SourceMode::Internal);
        if source == SourceMode::Internal { return; }
        let Ok(expected) = self.checker.resolve_type_expr(annotation, value.span) else { return; };
        if crate::semantics::conversions::implicit_contract(&expected).is_none() { return; }
        let owner = self.checker.current_class.clone();
        let parent = owner.as_ref().and_then(|name| self.checker.classes.get(name))
            .and_then(|class| class.parent.clone());
        let Ok(normalized) = super::normalize_lexical_callable_default(
            &Some(value.clone()), owner.as_deref(), parent.as_deref()) else { return; };
        // An unresolved trait receiver or lazy default retains the existing validation path.
        let warning_count = self.checker.warnings.len();
        let actual = self.checker.resolved_declaration_default_type(normalized.as_ref());
        self.checker.warnings.truncate(warning_count);
        let Ok(Some(actual)) = actual else { return; };
        if let Err(error) = self.checker.require_noncoercive_value_type(&expected, &actual,
            source, value.span, "Declaration default") {
            self.errors.push(error);
        }
    }
}
