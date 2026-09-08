//! Purpose:
//! Answers whether a statement list or an expression contains any node the CHECKER filed a
//! local-binding decision against, so no pass that CLONES an AST node ever duplicates one.
//!
//! Called from:
//! - `crate::optimize::control::dce`, as the third "must stay singular" guard on tail-sinking.
//! - `crate::optimize::control::switch`, which vetoes the single-case switch rewrite when it
//!   would emit a decision-carrying node in both branches of the synthesized `if`.
//!
//! Key details:
//! - `CheckResult::local_bind_kill_sites` and `local_retype_sites` are keyed BY SPAN, and EIR
//!   lowering consults them by span at every `unset` argument and every assignment. Tail-sinking
//!   copies the tail of an `if`/`switch`/`try` into EVERY branch, and a copy carries the ORIGINAL's
//!   spans — so both copies answer to the one decision. Abandoning a binding is not idempotent
//!   (it releases the old value and re-binds the name to a fresh slot), so the second copy is
//!   lowered against the first copy's post-rebind state: a read that sits ABOVE the retype in
//!   source resolves to the FRESH slot. Measured as a hard `local load from PHP type Int as Str`
//!   backend error, and — with two retypes — as silently printing `|s` for `a1|s`.
//! - The scan tests the span of EVERY statement and EVERY expression rather than re-deriving which
//!   shapes carry a decision. That is what makes it complete: retype sites are statement spans and
//!   kill sites are expression spans, so any node whose span is in the set is caught whatever the
//!   checker's recording rule becomes. The exhaustive matches are deliberate — a new AST variant
//!   must fail to compile here rather than silently become "carries no decision", which would be a
//!   miscompile rather than a missed optimization.
//! - CLOSURE bodies are walked for completeness but cannot actually be hurt by the duplication:
//!   each clone is lowered as its OWN EIR function with its own frame and its own slot maps, so a
//!   re-bind inside one clone cannot be observed by the other.

use crate::optimize::binding_decisions::{
    has_local_binding_decisions, span_carries_local_binding_decision,
};
use crate::parser::ast::{CallableTarget, CatchClause, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};

/// Returns true when any node in `stmts` carries a checker local-binding decision.
///
/// Short-circuits on the overwhelmingly common case of a program the checker made no decision for,
/// which keeps the callers exactly as cheap as they were.
pub(crate) fn stmts_carry_local_binding_decision(stmts: &[Stmt]) -> bool {
    has_local_binding_decisions() && stmts.iter().any(stmt_carries_decision)
}

/// Returns true when `expr` or any node under it carries a checker local-binding decision.
///
/// The expression form exists for the passes that duplicate an EXPRESSION rather than a statement
/// — the single-case switch rewrite clones the switch subject once per case pattern — and
/// short-circuits on the same empty-set fast path.
pub(crate) fn expr_carries_local_binding_decision(expr: &Expr) -> bool {
    has_local_binding_decisions() && expr_carries_decision(expr)
}

/// Walks one statement. Exhaustive so a new `StmtKind` cannot default to "carries no decision".
fn stmt_carries_decision(stmt: &Stmt) -> bool {
    if span_carries_local_binding_decision(stmt.span) {
        return true;
    }
    match &stmt.kind {
        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_carries_decision(expr)
        }
        StmtKind::Assign { name: _, value } => expr_carries_decision(value),
        StmtKind::RefAssign { target: _, source } => expr_carries_decision(source),
        StmtKind::TypedAssign { type_expr: _, name: _, value } => expr_carries_decision(value),
        StmtKind::ConstDecl { name: _, value } => expr_carries_decision(value),
        StmtKind::StaticVar { name: _, init } => expr_carries_decision(init),
        StmtKind::ListUnpack { vars: _, value } => expr_carries_decision(value),
        StmtKind::ArrayAssign { array: _, index, value } => {
            expr_carries_decision(index) || expr_carries_decision(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_carries_decision(target) || expr_carries_decision(value)
        }
        StmtKind::ArrayPush { array: _, value } => expr_carries_decision(value),
        StmtKind::PropertyAssign { object, property: _, value } => {
            expr_carries_decision(object) || expr_carries_decision(value)
        }
        StmtKind::PropertyArrayPush { object, property: _, value } => {
            expr_carries_decision(object) || expr_carries_decision(value)
        }
        StmtKind::DynamicPropertyArrayPush {
            object,
            property,
            value,
        } => {
            expr_carries_decision(object)
                || expr_carries_decision(property)
                || expr_carries_decision(value)
        }
        StmtKind::PropertyArrayAssign { object, property: _, index, value } => {
            expr_carries_decision(object)
                || expr_carries_decision(index)
                || expr_carries_decision(value)
        }
        StmtKind::StaticPropertyAssign { receiver: _, property: _, value } => {
            expr_carries_decision(value)
        }
        StmtKind::StaticPropertyArrayPush { receiver: _, property: _, value } => {
            expr_carries_decision(value)
        }
        StmtKind::StaticPropertyArrayAssign { receiver: _, property: _, index, value } => {
            expr_carries_decision(index) || expr_carries_decision(value)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_carries_decision),
        StmtKind::Include { path, once: _, required: _ } => expr_carries_decision(path),
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { name: _, body }
        | StmtKind::IncludeOnceGuard { label: _, body } => stmts_carry_decision(body),
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_carries_decision(condition) || stmts_carry_decision(body)
        }
        StmtKind::For { init, condition, update, body } => {
            init.as_deref().is_some_and(stmt_carries_decision)
                || condition.as_ref().is_some_and(expr_carries_decision)
                || update.as_deref().is_some_and(stmt_carries_decision)
                || stmts_carry_decision(body)
        }
        StmtKind::Foreach { array, key_var: _, value_var: _, value_by_ref: _, body } => {
            expr_carries_decision(array) || stmts_carry_decision(body)
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr_carries_decision(condition)
                || stmts_carry_decision(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(condition, body)| {
                        expr_carries_decision(condition) || stmts_carry_decision(body)
                    })
                || else_body.as_ref().is_some_and(|body| stmts_carry_decision(body))
        }
        StmtKind::IfDef { symbol: _, then_body, else_body } => {
            stmts_carry_decision(then_body)
                || else_body.as_ref().is_some_and(|body| stmts_carry_decision(body))
        }
        StmtKind::Switch { subject, cases, default } => {
            expr_carries_decision(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_carries_decision) || stmts_carry_decision(body)
                })
                || default.as_ref().is_some_and(|body| stmts_carry_decision(body))
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            stmts_carry_decision(try_body)
                || catches.iter().any(|CatchClause { exception_types: _, variable: _, body }| {
                    stmts_carry_decision(body)
                })
                || finally_body.as_ref().is_some_and(|body| stmts_carry_decision(body))
        }
        StmtKind::FunctionVariantGroup { name: _, variants: _ }
        | StmtKind::FunctionVariantMark { name: _, variant: _ }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { vars: _ }
        | StmtKind::IncludeOnceMark { label: _ }
        | StmtKind::NamespaceDecl { name: _ }
        | StmtKind::UseDecl { imports: _ }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

/// Walks a block without re-checking whether any decision exists at all.
fn stmts_carry_decision(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_carries_decision)
}

/// Walks a list of expressions.
fn exprs_carry_decision(exprs: &[Expr]) -> bool {
    exprs.iter().any(expr_carries_decision)
}

/// Walks one expression. Exhaustive for the same reason as `stmt_carries_decision`.
///
/// The span test here is what catches `unset` KILL sites: the checker files those against the
/// ARGUMENT expression's span, not the enclosing statement's.
fn expr_carries_decision(expr: &Expr) -> bool {
    if span_carries_local_binding_decision(expr.span) {
        return true;
    }
    match &expr.kind {
        ExprKind::Assignment { target, value, result_target, prelude, conditional_value_temp: _ } => {
            expr_carries_decision(target)
                || expr_carries_decision(value)
                || result_target.as_ref().is_some_and(|expr| expr_carries_decision(expr))
                || stmts_carry_decision(prelude)
        }
        ExprKind::FunctionCall { name: _, args } => exprs_carry_decision(args),
        ExprKind::MethodCall { object, method: _, args }
        | ExprKind::NullsafeMethodCall { object, method: _, args } => {
            expr_carries_decision(object) || exprs_carry_decision(args)
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            expr_carries_decision(object)
                || expr_carries_decision(method)
                || exprs_carry_decision(args)
        }
        ExprKind::StaticMethodCall { receiver: _, method: _, args }
        | ExprKind::NewScopedObject { receiver: _, args }
        | ExprKind::NewObject { class_name: _, args } => exprs_carry_decision(args),
        ExprKind::ClosureCall { var: _, args } => exprs_carry_decision(args),
        ExprKind::ExprCall { callee, args } => {
            expr_carries_decision(callee) || exprs_carry_decision(args)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_carries_decision(name_expr) || exprs_carry_decision(args)
        }
        ExprKind::NewDynamicObject { class_name, fallback_class: _, required_parent: _, args } => {
            expr_carries_decision(class_name) || exprs_carry_decision(args)
        }
        ExprKind::Pipe { value, callable } => {
            expr_carries_decision(value) || expr_carries_decision(callable)
        }
        ExprKind::IncludeValue { path, once: _, required: _ } => expr_carries_decision(path),
        ExprKind::Yield { key, value } => {
            key.as_ref().is_some_and(|expr| expr_carries_decision(expr))
                || value.as_ref().is_some_and(|expr| expr_carries_decision(expr))
        }
        ExprKind::Closure {
            params,
            variadic: _,
            variadic_type: _,
            return_type: _,
            body,
            is_arrow: _,
            is_static: _,
            by_ref_return: _,
            variadic_by_ref: _,
            captures: _,
            capture_refs: _,
        } => {
            params
                .iter()
                .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_carries_decision))
                || stmts_carry_decision(body)
        }
        ExprKind::BinaryOp { left, op: _, right } => {
            expr_carries_decision(left) || expr_carries_decision(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_carries_decision(value)
                || match target {
                    InstanceOfTarget::Expr(target) => expr_carries_decision(target),
                    InstanceOfTarget::Name(_) => false,
                }
        }
        ExprKind::YieldFrom(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { target: _, expr: inner }
        | ExprKind::PtrCast { target_type: _, expr: inner }
        | ExprKind::NamedArg { name: _, value: inner }
        | ExprKind::BufferNew { element_type: _, len: inner } => expr_carries_decision(inner),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_carries_decision(value) || expr_carries_decision(default)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr_carries_decision(condition)
                || expr_carries_decision(then_expr)
                || expr_carries_decision(else_expr)
        }
        ExprKind::Match { subject, arms, default } => {
            expr_carries_decision(subject)
                || arms.iter().any(|(patterns, body)| {
                    exprs_carry_decision(patterns) || expr_carries_decision(body)
                })
                || default.as_ref().is_some_and(|expr| expr_carries_decision(expr))
        }
        ExprKind::ArrayLiteral(items) => exprs_carry_decision(items),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_carries_decision(key) || expr_carries_decision(value)),
        ExprKind::ArrayAccess { array, index } => {
            expr_carries_decision(array) || expr_carries_decision(index)
        }
        ExprKind::PropertyAccess { object, property: _ }
        | ExprKind::NullsafePropertyAccess { object, property: _ }
        | ExprKind::ObjectClassName { object } => expr_carries_decision(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_carries_decision(object) || expr_carries_decision(property)
        }
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Method { object, method: _ } => expr_carries_decision(object),
            _ => false,
        },
        ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}
