//! Purpose:
//! Tracks the leading top-level declare sequence and the main script's strict mode.
//! Keeps statement consumption in the ordinary parser, including bare declare bodies.
//!
//! Called from:
//! - `crate::parser::stmt::parse_file_or_namespace_scope_item()`,
//!   `crate::parser::parse_checked_tokens()` and `crate::parser::stmt::declare`.
//!
//! Key details:
//! - Nested directives never inherit a top-level placement allowance.
//! - Main-script statements share the final strict flag, including the assignment preludes
//!   inside their expressions; earlier callable bodies keep theirs.

use std::cell::Cell;

use crate::errors::CompileError;
use crate::lexer::SpannedToken;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};

thread_local! {
    static STRICT_TYPES_POSITION: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Restores the enclosing parser's placement allowance, including during error unwinding.
struct PositionGuard(Option<usize>);

impl Drop for PositionGuard {
    /// Restores a saved allowance when this top-level statement parse ends.
    fn drop(&mut self) {
        STRICT_TYPES_POSITION.with(|position| position.set(self.0));
    }
}

/// Parses one top-level statement with an allowance for this exact leading declare token.
pub(super) fn parse_top_level_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    allows_strict_types: bool,
) -> Result<Stmt, CompileError> {
    let allowed = allows_strict_types.then_some(*pos);
    let _guard = PositionGuard(STRICT_TYPES_POSITION.with(|position| position.replace(allowed)));
    super::stmt::parse_stmt(tokens, pos)
}

/// Accepts a strict directive only at the top-level token currently being parsed.
pub(super) fn allows_strict_types(pos: usize) -> bool {
    STRICT_TYPES_POSITION.with(|position| position.get() == Some(pos))
}

/// Applies the main op-array's final strict flag without changing earlier callable scopes.
pub(super) fn mark_main_strict_types(stmts: &mut [Stmt]) {
    for stmt in stmts {
        mark_main_stmt_strict_types(stmt);
    }
}

/// Visits statement bodies and expressions in the main script, stopping at function and
/// class declarations.
fn mark_main_stmt_strict_types(stmt: &mut Stmt) {
    if matches!(stmt.kind, StmtKind::FunctionDecl { .. } | StmtKind::ClassDecl { .. }
        | StmtKind::TraitDecl { .. } | StmtKind::InterfaceDecl { .. } | StmtKind::EnumDecl { .. })
    {
        return;
    }
    stmt.strict_types = true;
    match &mut stmt.kind {
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => mark_main_strict_types(body),
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition }
        | StmtKind::Foreach { array: condition, body, .. } => {
            mark_main_expr_strict_types(condition);
            mark_main_strict_types(body);
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            mark_main_expr_strict_types(condition);
            mark_main_strict_types(then_body);
            for (condition, body) in elseif_clauses {
                mark_main_expr_strict_types(condition);
                mark_main_strict_types(body);
            }
            if let Some(body) = else_body { mark_main_strict_types(body); }
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            mark_main_strict_types(then_body);
            if let Some(body) = else_body { mark_main_strict_types(body); }
        }
        StmtKind::For { init, condition, update, body } => {
            if let Some(stmt) = init { mark_main_stmt_strict_types(stmt); }
            if let Some(condition) = condition { mark_main_expr_strict_types(condition); }
            if let Some(stmt) = update { mark_main_stmt_strict_types(stmt); }
            mark_main_strict_types(body);
        }
        StmtKind::Switch { subject, cases, default } => {
            mark_main_expr_strict_types(subject);
            for (conditions, body) in cases {
                conditions.iter_mut().for_each(mark_main_expr_strict_types);
                mark_main_strict_types(body);
            }
            if let Some(body) = default { mark_main_strict_types(body); }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            mark_main_strict_types(try_body);
            for catch in catches { mark_main_strict_types(&mut catch.body); }
            if let Some(body) = finally_body { mark_main_strict_types(body); }
        }
        StmtKind::Echo(value) | StmtKind::Throw(value) | StmtKind::ExprStmt(value)
        | StmtKind::Return(Some(value)) | StmtKind::Assign { value, .. }
        | StmtKind::RefAssign { source: value, .. } | StmtKind::ArrayPush { value, .. }
        | StmtKind::TypedAssign { value, .. } | StmtKind::Include { path: value, .. }
        | StmtKind::ConstDecl { value, .. } | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. } | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => mark_main_expr_strict_types(value),
        StmtKind::ArrayAssign { index: first, value: second, .. }
        | StmtKind::NestedArrayAssign { target: first, value: second }
        | StmtKind::StaticPropertyArrayAssign { index: first, value: second, .. } => {
            mark_main_expr_strict_types(first);
            mark_main_expr_strict_types(second);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            mark_main_expr_strict_types(object);
            mark_main_expr_strict_types(value);
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            mark_main_expr_strict_types(object);
            mark_main_expr_strict_types(index);
            mark_main_expr_strict_types(value);
        }
        StmtKind::Return(None) | StmtKind::IncludeOnceMark { .. } | StmtKind::Break(_)
        | StmtKind::Continue(_) | StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. } | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. } | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. } | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } | StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. } | StmtKind::EnumDecl { .. } | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. } => {}
    }
}

/// Marks the assignment preludes an expression runs in the main script.
///
/// The parser builds a prelude before it reaches a later strict directive, and the checker and
/// lowering apply each prelude statement's own flag. Closure bodies keep their lexical flag.
fn mark_main_expr_strict_types(expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::Reference(inner) => mark_main_expr_strict_types(inner),
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            mark_main_strict_types(prelude);
            mark_main_expr_strict_types(target);
            mark_main_expr_strict_types(value);
            if let Some(result_target) = result_target { mark_main_expr_strict_types(result_target); }
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce { value: left, default: right }
        | ExprKind::ShortTernary { value: left, default: right }
        | ExprKind::Pipe { value: left, callable: right }
        | ExprKind::ArrayAccess { array: left, index: right }
        | ExprKind::DynamicPropertyAccess { object: left, property: right }
        | ExprKind::NullsafeDynamicPropertyAccess { object: left, property: right } => {
            mark_main_expr_strict_types(left);
            mark_main_expr_strict_types(right);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            mark_main_expr_strict_types(condition);
            mark_main_expr_strict_types(then_expr);
            mark_main_expr_strict_types(else_expr);
        }
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner) | ExprKind::ErrorSuppress(inner) | ExprKind::Print(inner)
        | ExprKind::Spread(inner) | ExprKind::Clone(inner) | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. } | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. } | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::FirstClassCallable(CallableTarget::Method { object: inner, .. }) => {
            mark_main_expr_strict_types(inner);
        }
        ExprKind::InstanceOf { value, target } => {
            mark_main_expr_strict_types(value);
            if let InstanceOfTarget::Expr(target) = target { mark_main_expr_strict_types(target); }
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. } | ExprKind::NewGeneric { args, .. }
        | ExprKind::StaticMethodCall { args, .. } | ExprKind::NewScopedObject { args, .. }
        | ExprKind::ArrayLiteral(args) => args.iter_mut().for_each(mark_main_expr_strict_types),
        ExprKind::ExprCall { callee: target, args } | ExprKind::NewDynamic { name_expr: target, args }
        | ExprKind::NewDynamicObject { class_name: target, args, .. }
        | ExprKind::MethodCall { object: target, args, .. }
        | ExprKind::NullsafeMethodCall { object: target, args, .. } => {
            mark_main_expr_strict_types(target);
            args.iter_mut().for_each(mark_main_expr_strict_types);
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            mark_main_expr_strict_types(object);
            mark_main_expr_strict_types(method);
            args.iter_mut().for_each(mark_main_expr_strict_types);
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                mark_main_expr_strict_types(key);
                mark_main_expr_strict_types(value);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries { entry.exprs_mut().for_each(mark_main_expr_strict_types); }
        }
        ExprKind::Match { subject, arms, default } => {
            mark_main_expr_strict_types(subject);
            for (conditions, value) in arms {
                conditions.iter_mut().for_each(mark_main_expr_strict_types);
                mark_main_expr_strict_types(value);
            }
            if let Some(default) = default { mark_main_expr_strict_types(default); }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key { mark_main_expr_strict_types(key); }
            if let Some(value) = value { mark_main_expr_strict_types(value); }
        }
        ExprKind::Closure { .. } | ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_) | ExprKind::Variable(_) | ExprKind::BoolLiteral(_)
        | ExprKind::Null | ExprKind::PreIncrement(_) | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_) | ExprKind::PostDecrement(_) | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. } | ExprKind::FirstClassCallable(_)
        | ExprKind::This | ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
    }
}
