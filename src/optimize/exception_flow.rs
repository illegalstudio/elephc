//! Purpose:
//! Computes exception-type flow for AST dead-code elimination and catch routing.
//! Tracks exact thrown classes, constrained unknown throwable domains, and caught-variable rethrows.
//!
//! Called from:
//! - `crate::optimize::PostTypecheckOptimizer`
//! - `crate::optimize::control::dce::tries`
//!
//! Key details:
//! - Handler routing follows PHP first-match semantics and the checker-provided class hierarchy.
//! - Unknown call/runtime throws stay conservative while exact explicit throws remain precise.

use super::*;
use crate::types::{ClassInfo, InterfaceInfo};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod callables;
mod hierarchy;
mod types;

use callables::{
    attach_class_contexts, collect_exception_bodies, ExceptionBody, ExceptionClassContext,
};
use hierarchy::ExceptionHierarchy;
use types::{
    domain_can_match, domain_is_empty, domains_overlap, exact_in_domain, intersect_domain,
};
pub(super) use types::{CatchReachability, ThrownTypes};

/// Maximum callable-summary rounds before analysis falls back to fully unknown throws.
const MAX_EXCEPTION_SUMMARY_ITERATIONS: usize = 128;

thread_local! {
    /// Exception-flow analysis installed while post-typecheck DCE walks the AST.
    static ACTIVE_EXCEPTION_FLOW: RefCell<Option<Rc<ExceptionFlowAnalysis>>> = const { RefCell::new(None) };
    /// Exact/constrained throwable domains bound to catch variables in the current DCE region.
    static ACTIVE_CAUGHT_THROW_BINDINGS: RefCell<HashMap<String, ThrownTypes>> = RefCell::new(HashMap::new());
}

/// Fixed-point exception summaries and hierarchy shared by post-typecheck DCE.
#[derive(Clone, Debug)]
pub(super) struct ExceptionFlowAnalysis {
    hierarchy: ExceptionHierarchy,
    function_throws: HashMap<String, ThrownTypes>,
    static_method_throws: HashMap<String, ThrownTypes>,
    instance_method_throws: HashMap<String, ThrownTypes>,
}

/// Installs exception summaries for one optimizer pass and restores the previous analysis.
pub(super) fn with_exception_flow_analysis<R>(
    analysis: &Rc<ExceptionFlowAnalysis>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_EXCEPTION_FLOW.with(|slot| {
        let previous = slot.replace(Some(Rc::clone(analysis)));
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Installs one catch-variable throwable domain while recursively optimizing its body.
pub(super) fn with_caught_throw_binding<R>(
    variable: Option<&str>,
    incoming: &ThrownTypes,
    f: impl FnOnce() -> R,
) -> R {
    let Some(variable) = variable else {
        return f();
    };
    ACTIVE_CAUGHT_THROW_BINDINGS.with(|slot| {
        let mut next = slot.borrow().clone();
        next.insert(variable.to_string(), incoming.clone());
        let previous = slot.replace(next);
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Clones the currently active caught-variable domains for a local analysis query.
fn active_caught_throw_bindings() -> HashMap<String, ThrownTypes> {
    ACTIVE_CAUGHT_THROW_BINDINGS.with(|slot| slot.borrow().clone())
}

/// Mirrors the effect analysis's lexical class context for local DCE exception queries.
fn active_exception_class_context() -> Option<ExceptionClassContext> {
    ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
        slot.borrow().as_ref().map(|context| ExceptionClassContext {
            class_name: context.class_name.clone(),
            parent_name: context.parent_name.clone(),
        })
    })
}

/// Computes catch reachability through the active post-typecheck exception analysis.
pub(super) fn active_catch_reachability(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
) -> Vec<CatchReachability> {
    ACTIVE_EXCEPTION_FLOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|analysis| {
                let class_context = active_exception_class_context();
                analysis.catch_reachability(
                    try_body,
                    catches,
                    &active_caught_throw_bindings(),
                    class_context.as_ref(),
                )
            })
            .unwrap_or_else(|| {
                catches
                    .iter()
                    .map(|_| CatchReachability {
                        incoming: ThrownTypes::unknown(),
                    })
                    .collect()
            })
    })
}

/// Computes escaping types for one statement with no enclosing caught-variable binding.
pub(super) fn active_stmt_thrown_types(stmt: &Stmt) -> ThrownTypes {
    ACTIVE_EXCEPTION_FLOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|analysis| {
                let class_context = active_exception_class_context();
                analysis.stmt_throws(
                    stmt,
                    &active_caught_throw_bindings(),
                    class_context.as_ref(),
                )
            })
            .unwrap_or_else(|| {
                if stmt_effect(stmt).may_throw {
                    ThrownTypes::unknown()
                } else {
                    ThrownTypes::default()
                }
            })
    })
}

/// Computes escaping types for one expression with no enclosing caught-variable binding.
pub(super) fn active_expr_thrown_types(expr: &Expr) -> ThrownTypes {
    ACTIVE_EXCEPTION_FLOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|analysis| {
                let class_context = active_exception_class_context();
                analysis.expr_throws(
                    expr,
                    &active_caught_throw_bindings(),
                    class_context.as_ref(),
                )
            })
            .unwrap_or_else(|| {
                if expr_effect(expr).may_throw {
                    ThrownTypes::unknown()
                } else {
                    ThrownTypes::default()
                }
            })
    })
}

/// Returns whether two active throwable summaries can describe a common runtime value.
pub(super) fn active_thrown_types_overlap(left: &ThrownTypes, right: &ThrownTypes) -> bool {
    ACTIVE_EXCEPTION_FLOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|analysis| analysis.summaries_overlap(left, right))
            .unwrap_or_else(|| !left.is_empty() && !right.is_empty())
    })
}

/// Invalidates active catch-variable domains after DCE keeps a statement that may rewrite them.
pub(super) fn invalidate_active_caught_throw_bindings_for_stmt(stmt: &Stmt) {
    ACTIVE_CAUGHT_THROW_BINDINGS.with(|slot| match stmt_invalidation(stmt) {
        Invalidation::Names(names) => {
            let mut bindings = slot.borrow_mut();
            for name in names {
                bindings.remove(&name);
            }
        }
        Invalidation::All => slot.borrow_mut().clear(),
    });
}

impl ExceptionFlowAnalysis {
    /// Computes exception summaries using checker metadata when available.
    pub(super) fn from_program(
        program: &[Stmt],
        type_metadata: Option<(
            &HashMap<String, ClassInfo>,
            &HashMap<String, InterfaceInfo>,
        )>,
    ) -> Self {
        let mut function_bodies = HashMap::new();
        let mut static_method_bodies = HashMap::new();
        let mut instance_method_bodies = HashMap::new();
        let mut class_contexts = HashMap::new();
        collect_exception_bodies(
            program,
            &mut function_bodies,
            &mut static_method_bodies,
            &mut instance_method_bodies,
            &mut class_contexts,
        );
        let declared_classes = class_contexts.keys().cloned().collect();
        let mut hierarchy = if let Some((classes, interfaces)) = type_metadata {
            ExceptionHierarchy::from_type_metadata(classes, interfaces, declared_classes)
        } else {
            ExceptionHierarchy::from_program(program, declared_classes)
        };
        hierarchy.collect_program_declarations(program);
        let function_bodies = attach_class_contexts(function_bodies, &class_contexts);
        let static_method_bodies = attach_class_contexts(static_method_bodies, &class_contexts);
        let instance_method_bodies = attach_class_contexts(instance_method_bodies, &class_contexts);
        let mut analysis = Self {
            hierarchy,
            function_throws: empty_summaries(function_bodies.keys()),
            static_method_throws: empty_summaries(static_method_bodies.keys()),
            instance_method_throws: empty_summaries(instance_method_bodies.keys()),
        };

        for _ in 0..MAX_EXCEPTION_SUMMARY_ITERATIONS {
            let next_functions = analysis.summarize_bodies(&function_bodies);
            let next_static_methods = analysis.summarize_bodies(&static_method_bodies);
            let next_instance_methods = analysis.summarize_bodies(&instance_method_bodies);
            if next_functions == analysis.function_throws
                && next_static_methods == analysis.static_method_throws
                && next_instance_methods == analysis.instance_method_throws
            {
                return analysis;
            }
            analysis.function_throws = next_functions;
            analysis.static_method_throws = next_static_methods;
            analysis.instance_method_throws = next_instance_methods;
        }

        for summary in analysis
            .function_throws
            .values_mut()
            .chain(analysis.static_method_throws.values_mut())
            .chain(analysis.instance_method_throws.values_mut())
        {
            *summary = ThrownTypes::unknown();
        }
        analysis
    }

    /// Recomputes all summaries for one callable category from the preceding fixed-point state.
    fn summarize_bodies(&self, bodies: &HashMap<String, ExceptionBody<'_>>) -> HashMap<String, ThrownTypes> {
        bodies
            .iter()
            .map(|(name, body)| {
                (
                    name.clone(),
                    self.block_throws(body.body, &HashMap::new(), body.class_context),
                )
            })
            .collect()
    }

    /// Computes escaping throwables for a statement block under catch-variable bindings.
    fn block_throws(
        &self,
        stmts: &[Stmt],
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        let mut current_bindings = bindings.clone();
        let mut thrown = ThrownTypes::default();
        for stmt in stmts {
            thrown = thrown.combined(self.stmt_throws(stmt, &current_bindings, class_context));
            self.advance_throw_bindings_after_stmt(
                stmt,
                &mut current_bindings,
                class_context,
            );
            if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
                break;
            }
        }
        thrown
    }

    /// Invalidates caught-value domains after writes and records simple throwable aliases/rebinds.
    fn advance_throw_bindings_after_stmt(
        &self,
        stmt: &Stmt,
        bindings: &mut HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) {
        let replacement = match &stmt.kind {
            StmtKind::Assign { name, value }
            | StmtKind::TypedAssign { name, value, .. }
            | StmtKind::StaticVar {
                name,
                init: value,
            } => Some((
                name.clone(),
                self.thrown_value_types(value, bindings, class_context),
            )),
            StmtKind::ExprStmt(Expr {
                kind:
                    ExprKind::Assignment {
                        target,
                        value,
                        ..
                    },
                ..
            }) => match &target.kind {
                ExprKind::Variable(name) => Some((
                    name.clone(),
                    self.thrown_value_types(value, bindings, class_context),
                )),
                _ => None,
            },
            _ => None,
        };
        match stmt_invalidation(stmt) {
            Invalidation::Names(names) => {
                for name in names {
                    bindings.remove(&name);
                }
            }
            Invalidation::All => bindings.clear(),
        }
        if let Some((name, thrown)) = replacement {
            bindings.insert(name, thrown);
        }
    }

    /// Computes escaping throwables for one statement, recursively routing nested handlers.
    fn stmt_throws(
        &self,
        stmt: &Stmt,
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &stmt.kind {
            StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => {
                self.block_throws(body, bindings, class_context)
            }
            StmtKind::Echo(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::ConstDecl { value: expr, .. }
            | StmtKind::StaticVar { init: expr, .. }
            | StmtKind::ListUnpack { value: expr, .. }
            | StmtKind::Return(Some(expr)) => self.expr_throws(expr, bindings, class_context),
            StmtKind::Throw(expr) => self
                .expr_throws(expr, bindings, class_context)
                .combined(self.thrown_value_types(expr, bindings, class_context)),
            StmtKind::Assign { value, .. }
            | StmtKind::TypedAssign { value, .. }
            | StmtKind::StaticPropertyAssign { value, .. } => {
                self.expr_throws(value, bindings, class_context)
            }
            StmtKind::ArrayPush { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => self
                .expr_throws(value, bindings, class_context)
                .combined(ThrownTypes::unknown()),
            StmtKind::ArrayAssign { index, value, .. }
            | StmtKind::PropertyArrayAssign { index, value, .. }
            | StmtKind::StaticPropertyArrayAssign { index, value, .. } => self
                .expr_throws(index, bindings, class_context)
                .combined(self.expr_throws(value, bindings, class_context))
                .combined(ThrownTypes::unknown()),
            StmtKind::NestedArrayAssign { target, value } => self
                .expr_throws(target, bindings, class_context)
                .combined(self.expr_throws(value, bindings, class_context))
                .combined(ThrownTypes::unknown()),
            StmtKind::PropertyAssign {
                object: target,
                value,
                ..
            }
            | StmtKind::PropertyArrayPush {
                object: target,
                value,
                ..
            } => self
                .expr_throws(target, bindings, class_context)
                .combined(self.expr_throws(value, bindings, class_context))
                .combined(ThrownTypes::unknown()),
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                let mut thrown = self.expr_throws(condition, bindings, class_context);
                thrown = thrown.combined(self.block_throws(then_body, bindings, class_context));
                for (condition, body) in elseif_clauses {
                    thrown = thrown
                        .combined(self.expr_throws(condition, bindings, class_context))
                        .combined(self.block_throws(body, bindings, class_context));
                }
                if let Some(body) = else_body {
                    thrown = thrown.combined(self.block_throws(body, bindings, class_context));
                }
                thrown
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                let mut thrown = self.block_throws(then_body, bindings, class_context);
                if let Some(body) = else_body {
                    thrown = thrown.combined(self.block_throws(body, bindings, class_context));
                }
                thrown
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => self
                .expr_throws(condition, bindings, class_context)
                .combined(self.block_throws(body, bindings, class_context)),
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => init
                .as_deref()
                .map(|stmt| self.stmt_throws(stmt, bindings, class_context))
                .unwrap_or_default()
                .combined(
                    condition
                        .as_ref()
                        .map(|expr| self.expr_throws(expr, bindings, class_context))
                        .unwrap_or_default(),
                )
                .combined(
                    update
                        .as_deref()
                        .map(|stmt| self.stmt_throws(stmt, bindings, class_context))
                        .unwrap_or_default(),
                )
                .combined(self.block_throws(body, bindings, class_context)),
            StmtKind::Foreach { array, body, .. } => self
                .expr_throws(array, bindings, class_context)
                .combined(self.block_throws(body, bindings, class_context)),
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                let mut thrown = self.expr_throws(subject, bindings, class_context);
                for (patterns, body) in cases {
                    for pattern in patterns {
                        thrown = thrown.combined(self.expr_throws(pattern, bindings, class_context));
                    }
                    thrown = thrown.combined(self.block_throws(body, bindings, class_context));
                }
                if let Some(body) = default {
                    thrown = thrown.combined(self.block_throws(body, bindings, class_context));
                }
                thrown
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => self.try_throws(try_body, catches, finally_body, bindings, class_context),
            StmtKind::Include { .. } => ThrownTypes::unknown(),
            _ if stmt_effect(stmt).may_throw => ThrownTypes::unknown(),
            _ => ThrownTypes::default(),
        }
    }

    /// Routes nested try exceptions through first-match catches and applies finally override rules.
    fn try_throws(
        &self,
        try_body: &[Stmt],
        catches: &[crate::parser::ast::CatchClause],
        finally_body: &Option<Vec<Stmt>>,
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        let try_throws = self.block_throws(try_body, bindings, class_context);
        let (routes, mut escaping) = self.route_catches(try_throws, catches, class_context);
        for (catch, route) in catches.iter().zip(routes) {
            if !route.is_reachable() {
                continue;
            }
            let mut catch_bindings = bindings.clone();
            if let Some(variable) = &catch.variable {
                catch_bindings.insert(variable.clone(), route.incoming);
            }
            escaping = escaping.combined(self.block_throws(
                &catch.body,
                &catch_bindings,
                class_context,
            ));
        }
        let Some(finally_body) = finally_body else {
            return escaping;
        };
        let finally_throws = self.block_throws(finally_body, bindings, class_context);
        if matches!(
            block_terminal_effect(finally_body),
            TerminalEffect::FallsThrough
        ) {
            escaping.combined(finally_throws)
        } else {
            finally_throws
        }
    }

    /// Computes throwable flow for an expression, preserving exact explicit and callable throws.
    fn expr_throws(
        &self,
        expr: &Expr,
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &expr.kind {
            ExprKind::Throw(inner) => self
                .expr_throws(inner, bindings, class_context)
                .combined(self.thrown_value_types(inner, bindings, class_context)),
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::PtrCast { expr: inner, .. }
            | ExprKind::Spread(inner) => self.expr_throws(inner, bindings, class_context),
            ExprKind::Clone(inner) | ExprKind::YieldFrom(inner) => self
                .expr_throws(inner, bindings, class_context)
                .combined(ThrownTypes::unknown()),
            ExprKind::BinaryOp { left, op, right } => {
                let mut thrown = self
                    .expr_throws(left, bindings, class_context)
                    .combined(self.expr_throws(right, bindings, class_context));
                if let Some(class_name) = binary_op_exact_throw_type(left, op, right) {
                    thrown = thrown.combined(ThrownTypes::exact(class_name));
                } else if binary_op_has_dynamic_throw(left, op, right) {
                    thrown = thrown.combined(ThrownTypes::unknown());
                }
                thrown
            }
            ExprKind::FunctionCall { name, args } => {
                let mut thrown = self.expr_list_throws(args, bindings, class_context);
                if let Some(summary) = self.function_throws.get(name.as_str()) {
                    return thrown.combined(summary.clone());
                }
                if function_call_effect(name.as_str(), args).may_throw {
                    thrown = thrown.combined(ThrownTypes::unknown());
                }
                thrown
            }
            ExprKind::NewObject { class_name, args } => self
                .expr_list_throws(args, bindings, class_context)
                .combined(self.constructor_throws(class_name.as_str())),
            ExprKind::NewScopedObject { receiver, args } => {
                let mut thrown = self.expr_list_throws(args, bindings, class_context);
                if let Some(class_name) = resolve_exception_receiver(receiver, class_context) {
                    thrown = thrown.combined(self.constructor_throws(&class_name));
                } else {
                    thrown = thrown.combined(ThrownTypes::unknown());
                }
                thrown
            }
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                let mut thrown = self.expr_list_throws(args, bindings, class_context);
                if let Some(class_name) = resolve_exception_receiver(receiver, class_context) {
                    if let Some(summary) = self.resolve_method_summary(
                        &class_name,
                        method,
                        &self.static_method_throws,
                    ) {
                        return thrown.combined(summary);
                    }
                }
                if expr_effect(expr).may_throw {
                    thrown = thrown.combined(ThrownTypes::unknown());
                }
                thrown
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            }
            | ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => {
                let mut thrown = self
                    .expr_throws(object, bindings, class_context)
                    .combined(self.expr_list_throws(args, bindings, class_context));
                if let Some(class_name) = exact_receiver_class(object, class_context) {
                    if let Some(summary) = self.resolve_method_summary(
                        &class_name,
                        method,
                        &self.instance_method_throws,
                    ) {
                        return thrown.combined(summary);
                    }
                }
                if expr_effect(expr).may_throw {
                    thrown = thrown.combined(ThrownTypes::unknown());
                }
                thrown
            }
            ExprKind::ExprCall { callee, args } => self
                .expr_throws(callee, bindings, class_context)
                .combined(self.expr_list_throws(args, bindings, class_context))
                .combined(self.callable_expr_throws(callee, bindings, class_context)),
            ExprKind::Pipe { value, callable } => self
                .expr_throws(value, bindings, class_context)
                .combined(self.expr_throws(callable, bindings, class_context))
                .combined(self.callable_expr_throws(callable, bindings, class_context)),
            ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => ThrownTypes::default(),
            ExprKind::ArrayLiteral(items) => self.expr_list_throws(items, bindings, class_context),
            ExprKind::ArrayLiteralAssoc(items) => items.iter().fold(
                ThrownTypes::default(),
                |thrown, (key, value)| {
                    thrown
                        .combined(self.expr_throws(key, bindings, class_context))
                        .combined(self.expr_throws(value, bindings, class_context))
                },
            ),
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => self
                .expr_throws(condition, bindings, class_context)
                .combined(self.expr_throws(then_expr, bindings, class_context))
                .combined(self.expr_throws(else_expr, bindings, class_context)),
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => self
                .expr_throws(value, bindings, class_context)
                .combined(self.expr_throws(default, bindings, class_context)),
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                let mut thrown = self.expr_throws(subject, bindings, class_context);
                for (patterns, value) in arms {
                    thrown = thrown
                        .combined(self.expr_list_throws(patterns, bindings, class_context))
                        .combined(self.expr_throws(value, bindings, class_context));
                }
                if let Some(default) = default {
                    thrown = thrown.combined(self.expr_throws(default, bindings, class_context));
                }
                thrown
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => self
                .block_throws(prelude, bindings, class_context)
                .combined(self.expr_throws(target, bindings, class_context))
                .combined(self.expr_throws(value, bindings, class_context))
                .combined(
                    result_target
                        .as_deref()
                        .map(|expr| self.expr_throws(expr, bindings, class_context))
                        .unwrap_or_default(),
                ),
            _ if expr_effect(expr).may_throw => ThrownTypes::unknown(),
            _ => ThrownTypes::default(),
        }
    }

    /// Computes throws from a source-order expression list.
    fn expr_list_throws(
        &self,
        exprs: &[Expr],
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        exprs.iter().fold(ThrownTypes::default(), |thrown, expr| {
            thrown.combined(self.expr_throws(expr, bindings, class_context))
        })
    }

    /// Infers the throwable value supplied to `throw`, including caught-variable domains.
    fn thrown_value_types(
        &self,
        expr: &Expr,
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &expr.kind {
            ExprKind::NewObject { class_name, .. } => ThrownTypes::exact(class_name.as_str()),
            ExprKind::NewScopedObject { receiver, .. } => resolve_exception_receiver(receiver, class_context)
                .map(|class_name| ThrownTypes::exact(&class_name))
                .unwrap_or_else(ThrownTypes::unknown),
            ExprKind::Variable(name) => bindings
                .get(name)
                .cloned()
                .unwrap_or_else(ThrownTypes::unknown),
            ExprKind::ErrorSuppress(inner) => {
                self.thrown_value_types(inner, bindings, class_context)
            }
            ExprKind::Assignment { value, .. } => {
                self.thrown_value_types(value, bindings, class_context)
            }
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => self
                .thrown_value_types(then_expr, bindings, class_context)
                .combined(self.thrown_value_types(else_expr, bindings, class_context)),
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => self
                .thrown_value_types(value, bindings, class_context)
                .combined(self.thrown_value_types(default, bindings, class_context)),
            ExprKind::Match { arms, default, .. } => {
                let mut thrown = arms.iter().fold(ThrownTypes::default(), |thrown, (_, value)| {
                    thrown.combined(self.thrown_value_types(value, bindings, class_context))
                });
                if let Some(default) = default {
                    thrown = thrown.combined(self.thrown_value_types(default, bindings, class_context));
                }
                thrown
            }
            _ => ThrownTypes::unknown(),
        }
    }

    /// Resolves direct closure/FCC callable throws, retaining unknown aliases conservatively.
    fn callable_expr_throws(
        &self,
        callee: &Expr,
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &callee.kind {
            ExprKind::Closure { body, .. } => self.block_throws(body, bindings, class_context),
            ExprKind::FirstClassCallable(CallableTarget::Function(name)) => self
                .function_throws
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(ThrownTypes::unknown),
            _ => ThrownTypes::unknown(),
        }
    }

    /// Resolves constructor-body throws or conservatively classifies external builtin constructors.
    fn constructor_throws(&self, class_name: &str) -> ThrownTypes {
        if let Some(summary) = self.resolve_method_summary(
            class_name,
            "__construct",
            &self.instance_method_throws,
        ) {
            return summary;
        }
        if self
            .hierarchy
            .constructor_hierarchy_is_closed(class_name)
            && (self.hierarchy.is_declared_class(class_name)
                || self.hierarchy.is_subtype(class_name, "Throwable"))
        {
            ThrownTypes::default()
        } else {
            ThrownTypes::unknown()
        }
    }

    /// Resolves an inherited exact-class method summary through the parent chain.
    fn resolve_method_summary(
        &self,
        class_name: &str,
        method: &str,
        summaries: &HashMap<String, ThrownTypes>,
    ) -> Option<ThrownTypes> {
        let mut current = Some(class_name.to_string());
        let mut seen = HashSet::new();
        while let Some(class_name) = current {
            let class_key = php_symbol_key(&class_name);
            if !seen.insert(class_key.clone()) {
                return None;
            }
            let method_key = method_effect_key(&class_name, method);
            if let Some(summary) = summaries.get(&method_key) {
                return Some(summary.clone());
            }
            if self
                .hierarchy
                .class_has_trait_method_barrier(&class_name)
            {
                return None;
            }
            current = self.hierarchy.parents.get(&class_key).cloned();
        }
        None
    }

    /// Routes a throwable summary through source-order catch clauses and returns uncaught flow.
    fn route_catches(
        &self,
        mut remaining: ThrownTypes,
        catches: &[crate::parser::ast::CatchClause],
        class_context: Option<&ExceptionClassContext>,
    ) -> (Vec<CatchReachability>, ThrownTypes) {
        let mut routes = Vec::with_capacity(catches.len());
        for catch in catches {
            let handler_types: Vec<String> = catch
                .exception_types
                .iter()
                .map(|name| resolve_catch_handler_type(name.as_str(), class_context))
                .collect();
            let handler_refs: Vec<&str> = handler_types.iter().map(String::as_str).collect();
            let (incoming, next_remaining) =
                self.split_for_handlers(remaining, &handler_refs);
            routes.push(CatchReachability { incoming });
            remaining = next_remaining;
        }
        (routes, remaining)
    }

    /// Splits one throwable summary into values matched and not matched by a multi-catch.
    fn split_for_handlers(
        &self,
        thrown: ThrownTypes,
        handlers: &[&str],
    ) -> (ThrownTypes, ThrownTypes) {
        let mut matched = ThrownTypes::default();
        let mut remaining = ThrownTypes::default();
        for exact in thrown.exact {
            if handlers
                .iter()
                .any(|handler| self.hierarchy.is_subtype(&exact, handler))
            {
                matched.exact.insert(exact);
            } else {
                remaining.exact.insert(exact);
            }
        }
        for domain in thrown.domains {
            for handler in handlers {
                if domain_can_match(&self.hierarchy, &domain, handler) {
                    matched.domains.insert(intersect_domain(&self.hierarchy, &domain, handler));
                }
            }
            if !handlers
                .iter()
                .any(|handler| self.hierarchy.is_subtype(&domain.upper, handler))
            {
                let mut unmatched = domain;
                unmatched
                    .excluded
                    .extend(handlers.iter().map(|handler| (*handler).to_string()));
                if !domain_is_empty(&self.hierarchy, &unmatched) {
                    remaining.domains.insert(unmatched);
                }
            }
        }
        (matched, remaining)
    }

    /// Returns catch reachability for an already-optimized try body.
    fn catch_reachability(
        &self,
        try_body: &[Stmt],
        catches: &[crate::parser::ast::CatchClause],
        bindings: &HashMap<String, ThrownTypes>,
        class_context: Option<&ExceptionClassContext>,
    ) -> Vec<CatchReachability> {
        self.route_catches(
            self.block_throws(try_body, bindings, class_context),
            catches,
            class_context,
        )
        .0
    }

    /// Returns whether two throwable summaries have at least one possible common value.
    pub(super) fn summaries_overlap(&self, left: &ThrownTypes, right: &ThrownTypes) -> bool {
        left.exact.iter().any(|left_type| {
            right.exact.contains(left_type)
                || right
                    .domains
                    .iter()
                    .any(|domain| exact_in_domain(&self.hierarchy, left_type, domain))
        }) || right.exact.iter().any(|right_type| {
            left.domains
                .iter()
                .any(|domain| exact_in_domain(&self.hierarchy, right_type, domain))
        }) || left.domains.iter().any(|left_domain| {
            right.domains.iter().any(|right_domain| {
                domains_overlap(&self.hierarchy, left_domain, right_domain)
            })
        })
    }
}

/// Creates empty fixed-point summaries for a collection of callable names.
fn empty_summaries<'a>(names: impl Iterator<Item = &'a String>) -> HashMap<String, ThrownTypes> {
    names
        .map(|name| (name.clone(), ThrownTypes::default()))
        .collect()
}

/// Returns a guaranteed operator failure class when the right operand proves the failure.
fn binary_op_exact_throw_type(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
) -> Option<&'static str> {
    let left_is_numeric_literal = matches!(
        &left.kind,
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
    );
    match op {
        BinOp::Div | BinOp::Mod
            if left_is_numeric_literal
                && (matches!(&right.kind, ExprKind::IntLiteral(0))
                    || matches!(&right.kind, ExprKind::FloatLiteral(value) if *value == 0.0)) =>
        {
            Some("DivisionByZeroError")
        }
        BinOp::ShiftLeft | BinOp::ShiftRight
            if matches!(&left.kind, ExprKind::IntLiteral(_))
                && matches!(&right.kind, ExprKind::IntLiteral(value) if *value < 0) =>
        {
            Some("ArithmeticError")
        }
        _ => None,
    }
}

/// Returns whether an operator may fail but runtime operand types prevent an exact class claim.
fn binary_op_has_dynamic_throw(left: &Expr, op: &BinOp, right: &Expr) -> bool {
    match op {
        BinOp::Div | BinOp::Mod => !matches!(
            &right.kind,
            ExprKind::IntLiteral(value) if *value != 0
        ) && !matches!(
            &right.kind,
            ExprKind::FloatLiteral(value) if *value != 0.0
        ) && binary_op_exact_throw_type(left, op, right).is_none(),
        BinOp::ShiftLeft | BinOp::ShiftRight => !matches!(
            &right.kind,
            ExprKind::IntLiteral(value) if *value >= 0
        ) && binary_op_exact_throw_type(left, op, right).is_none(),
        _ => false,
    }
}

/// Resolves named/self/parent receivers; late-bound static remains non-exact.
fn resolve_exception_receiver(
    receiver: &crate::parser::ast::StaticReceiver,
    class_context: Option<&ExceptionClassContext>,
) -> Option<String> {
    match receiver {
        crate::parser::ast::StaticReceiver::Named(name) => Some(name.as_str().to_string()),
        crate::parser::ast::StaticReceiver::Self_ => {
            class_context.map(|context| context.class_name.clone())
        }
        crate::parser::ast::StaticReceiver::Parent => {
            class_context.and_then(|context| context.parent_name.clone())
        }
        crate::parser::ast::StaticReceiver::Static => None,
    }
}

/// Resolves `self` and `parent` catch types through the current lexical class context.
fn resolve_catch_handler_type(
    handler: &str,
    class_context: Option<&ExceptionClassContext>,
) -> String {
    match handler {
        "self" => class_context
            .map(|context| context.class_name.clone())
            .unwrap_or_else(|| handler.to_string()),
        "parent" => class_context
            .and_then(|context| context.parent_name.clone())
            .unwrap_or_else(|| handler.to_string()),
        _ => handler.to_string(),
    }
}

/// Resolves expression receiver forms whose runtime class is statically exact.
fn exact_receiver_class(
    object: &Expr,
    class_context: Option<&ExceptionClassContext>,
) -> Option<String> {
    match &object.kind {
        ExprKind::NewObject { class_name, .. } => Some(class_name.as_str().to_string()),
        ExprKind::NewScopedObject { receiver, .. } => {
            resolve_exception_receiver(receiver, class_context)
        }
        // `$this` can name a subclass instance whose override has a different
        // throw summary. Keep it unknown until dispatch metadata proves the
        // receiver or method is closed.
        ExprKind::This => None,
        _ => None,
    }
}
