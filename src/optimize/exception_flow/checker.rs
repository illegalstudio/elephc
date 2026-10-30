//! Purpose:
//! Shares typed exception routing with checker control-flow snapshots.
//!
//! Called from:
//! - Checker initialization and operation-boundary exception recording.
//!
//! Key details:
//! - Operand throws are recorded when the checker evaluates each operand.
//! - Opaque throwable domains retain first-match handler exclusions across nested tries.

use super::*;

/// Immutable callable summaries reused throughout a checker run.
pub(crate) struct CheckerExceptionAnalysis {
    effects: CallableEffectAnalysis,
    analysis: ExceptionFlowAnalysis,
}

/// Exact classes and constrained unknown domains crossing one control-flow edge.
#[derive(Clone)]
pub(crate) struct CheckerThrownTypes(ThrownTypes);

impl CheckerExceptionAnalysis {
    /// Computes summaries once before body checking, without requiring checked metadata.
    pub(crate) fn new(program: &Program) -> Self {
        let effects = CallableEffectAnalysis::from_program(program);
        let analysis = with_callable_effect_analysis(&effects, ||
            ExceptionFlowAnalysis::from_program(program, None));
        Self { effects, analysis }
    }

    /// Reports throws from this operation after its operands have already been evaluated.
    pub(crate) fn operation_throws(&self, expr: &Expr, class: Option<&str>) -> CheckerThrownTypes {
        // Process termination skips finally. Exceptions from an exit argument are recorded
        // separately when that argument is evaluated.
        if Self::is_process_exit(expr) {
            return CheckerThrownTypes(ThrownTypes::default());
        }
        let context = class.map(|name| ExceptionClassContext {
            class_name: name.to_string(),
            parent_name: self.analysis.hierarchy.parents.get(&php_symbol_key(name)).cloned(),
        });
        CheckerThrownTypes(with_callable_effect_analysis(&self.effects, ||
            self.analysis.expr_throws_at_boundary(expr, &HashMap::new(), context.as_ref(), false)))
    }

    /// Recognizes process exits whose argument effects were recorded separately.
    fn is_process_exit(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::ErrorSuppress(inner) => Self::is_process_exit(inner),
            ExprKind::FunctionCall { name, .. } => matches!(php_symbol_key(name.as_str()).as_str(), "exit" | "die"),
            _ => false,
        }
    }

    /// Reports callbacks and cleanup owned by a statement, excluding evaluated operands.
    pub(crate) fn statement_throws(&self, stmt: &Stmt, class: Option<&str>, retired_type: Option<&PhpType>) -> CheckerThrownTypes {
        if matches!(&stmt.kind, StmtKind::ExprStmt(expr) if Self::is_process_exit(expr)) {
            return CheckerThrownTypes(ThrownTypes::default());
        }
        let context = class.map(|name| ExceptionClassContext {
            class_name: name.to_string(),
            parent_name: self.analysis.hierarchy.parents.get(&php_symbol_key(name)).cloned(),
        });
        CheckerThrownTypes(with_callable_effect_analysis(&self.effects, ||
            self.analysis.stmt_throws_at_boundary(stmt, &HashMap::new(), context.as_ref(), false, retired_type)))
    }

    /// Routes one edge through a multi-catch while retaining the uncaught domain.
    pub(crate) fn split(&self, thrown: CheckerThrownTypes, handlers: &[String])
        -> (CheckerThrownTypes, CheckerThrownTypes)
    {
        let handlers: Vec<&str> = handlers.iter().map(String::as_str).collect();
        let (caught, remaining) = self.analysis.split_for_handlers(thrown.0, &handlers);
        (CheckerThrownTypes(caught), CheckerThrownTypes(remaining))
    }
}

impl CheckerThrownTypes {
    /// Keeps an explicitly thrown value conservative with respect to subclasses.
    pub(crate) fn from_value(ty: &PhpType, exact: bool) -> Self {
        Self(match ty {
            PhpType::Object(name) if exact => ThrownTypes::exact(name),
            PhpType::Object(name) => ThrownTypes::domain(name),
            _ => ThrownTypes::unknown(),
        })
    }

    /// Identifies operations with no exception edge.
    pub(crate) fn is_empty(&self) -> bool { self.0.is_empty() }
}
