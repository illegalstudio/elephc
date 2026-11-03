//! Purpose:
//! Shares typed exception routing with checker control-flow snapshots.
//!
//! Called from:
//! - Checker initialization and operation-boundary exception recording.
//! - Loop-entry stabilization when selecting reachable catch bindings.
//!
//! Key details:
//! - Operand throws are recorded when the checker evaluates each operand.
//! - Opaque throwable domains retain first-match handler exclusions across nested tries.
//! - Block queries include operands and retain the current lexical return-boundary profile.

use super::*;

/// Immutable callable summaries reused throughout a checker run.
pub(crate) struct CheckerExceptionAnalysis {
    effects: CallableEffectAnalysis,
    analysis: ExceptionFlowAnalysis,
    signatures: ByRefSignatures,
}

/// Exact classes and constrained unknown domains crossing one control-flow edge.
#[derive(Clone)]
pub(crate) struct CheckerThrownTypes(ThrownTypes);

impl CheckerExceptionAnalysis {
    /// Computes AST summaries with registered nominal parents before body checking.
    pub(crate) fn new(program: &Program, registered_parents: &[(String, String)]) -> Self {
        let effects = CallableEffectAnalysis::from_program(program);
        let analysis = with_callable_effect_analysis(&effects, ||
            ExceptionFlowAnalysis::from_program_with_registered_parents(program, None, registered_parents));
        Self { effects, analysis, signatures: collect_by_ref_signatures(program) }
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
    pub(crate) fn statement_throws(&self, stmt: &Stmt, class: Option<&str>, retired_type: Option<&PhpType>, reference_write: Option<bool>) -> CheckerThrownTypes {
        if matches!(&stmt.kind, StmtKind::ExprStmt(expr) if Self::is_process_exit(expr)) {
            return CheckerThrownTypes(ThrownTypes::default());
        }
        let context = class.map(|name| ExceptionClassContext {
            class_name: name.to_string(),
            parent_name: self.analysis.hierarchy.parents.get(&php_symbol_key(name)).cloned(),
        });
        CheckerThrownTypes(with_callable_effect_analysis(&self.effects, ||
            self.analysis.stmt_throws_at_boundary(stmt, &HashMap::new(), context.as_ref(), false, retired_type, reference_write)))
    }

    /// Reports destruction of a binding's old value before an implicit replacement.
    pub(crate) fn retirement_throws(&self, retired_type: &PhpType) -> CheckerThrownTypes {
        CheckerThrownTypes(self.analysis.type_destruction_throws(retired_type))
    }

    /// Creates an isolated query with loop-wide upper bounds for local retirement.
    pub(crate) fn catch_query(&self, types: crate::types::TypeEnv) -> CheckerCatchAnalysis<'_> {
        let mut analysis = self.analysis.clone();
        analysis.checker_retirement_types = Some(Rc::new(types));
        CheckerCatchAnalysis { effects: &self.effects, analysis }
    }

    /// Batches caller writes under shared signatures, including named and spread references.
    pub(crate) fn caller_writes(&self, expressions: &[&Expr], function_scope: bool) -> Option<HashSet<String>> {
        let collect = || with_callable_effect_analysis(&self.effects, ||
            with_by_ref_signatures(self.signatures.clone(), || {
                let mut written = HashSet::new();
                for expr in expressions {
                    // Deletion cannot introduce a destructible value. Nested operand calls
                    // are collected separately and retain their own invalidations.
                    if matches!(&expr.kind, ExprKind::FunctionCall { name, .. }
                        if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("unset"))
                    { continue; }
                    match expr_invalidation(expr) {
                        Invalidation::Names(names) => written.extend(names),
                        Invalidation::All => return None,
                    }
                }
                Some(written)
            }));
        if function_scope { with_function_scope(collect) } else { collect() }
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

/// A frame-local exception query whose retirement bounds never affect shared summaries.
pub(crate) struct CheckerCatchAnalysis<'a> {
    effects: &'a CallableEffectAnalysis,
    analysis: ExceptionFlowAnalysis,
}

impl CheckerCatchAnalysis<'_> {
    /// Identifies source-order handlers reachable from operands, calls, and typed cleanup.
    pub(crate) fn catch_reachability(
        &self, body: &[Stmt], catches: &[crate::parser::ast::CatchClause], class: Option<&str>,
    ) -> Vec<bool> {
        let context = class.map(|name| ExceptionClassContext {
            class_name: name.to_string(),
            parent_name: self.analysis.hierarchy.parents.get(&php_symbol_key(name)).cloned(),
        });
        with_callable_effect_analysis(self.effects, || {
            let thrown = self.analysis.block_throws(body, &HashMap::new(), context.as_ref());
            let (routes, _) = self.analysis.route_catches(thrown, catches, context.as_ref());
            routes.into_iter().map(|route| !route.incoming.is_empty()).collect()
        })
    }
}

impl ExceptionFlowAnalysis {
    /// Uses a proven local upper bound, retaining unknown cleanup for other target shapes.
    pub(super) fn local_retirement_throws(&self, target: &Expr) -> ThrownTypes {
        let ty = match &target.kind {
            ExprKind::Variable(name) => self.checker_retirement_types.as_ref()
                .and_then(|types| types.get(name)),
            _ => None,
        };
        ty.map_or_else(|| self.overwrite_cleanup_throws(), |ty| self.type_destruction_throws(ty))
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
