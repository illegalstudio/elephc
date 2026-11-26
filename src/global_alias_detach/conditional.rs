//! Purpose:
//! Path-sensitive half of the `global $x` alias pass: an `unset($GLOBALS['x'])` that only runs on
//! one control-flow path still detaches the alias in PHP, so the removed value must stay reachable
//! through the local reference without clearing the global on the other path.
//!
//! Called from:
//! - `crate::global_alias_detach::rewrite_body` when a body binds `global $x` and unsets it from a
//!   statement nested below the body's top level.
//!
//! Key details:
//! - The choice is made explicit at runtime: a hidden flag records whether the global was detached,
//!   reads become `$flag ? $hidden : $x`, and a write branches into the detached copy or the global
//!   slot instead of writing through a conditional expression (PHP rejects that as a write context).
//! - `isset($x)` and `unset($x)` never become conditional operands: `isset()` refuses an expression
//!   result and `unset()` needs an lvalue, so both are rewritten into the conditional itself.
//! - Detection is conservative. A shape the transform cannot express — a reference, a closure
//!   capture, destructuring, a loop variable, a nested declaration, a write through a value that is
//!   not rooted at the alias — abandons the whole body, leaving today's behavior rather than
//!   rewriting half of it.

use super::*;
use crate::span::Span;

/// Rewrites one body so a nested `unset($GLOBALS[$name])` detaches the alias at run time.
///
/// Returns `None` when the body's shape cannot be expressed, in which case the caller leaves the
/// body untouched.
pub(super) fn detach(mut statements: Vec<Stmt>, name: &str, counter: &mut usize) -> Option<Vec<Stmt>> {
    let declaration = statements.iter().position(|stmt| {
        matches!(&stmt.kind, StmtKind::Global { vars } if vars.iter().any(|var| var == name))
    })?;
    if super::reattaches_later(&statements[declaration + 1..], name) {
        return None;
    }
    let span = statements[declaration].span;
    let rewriter = ConditionalDetach {
        name: name.to_string(),
        hidden: format!("__elephc_detach_global_{}_{}", *counter, name),
        flag: format!("__elephc_detach_flag_{}_{}", *counter, name),
    };
    let mut tail: Vec<Stmt> = statements.split_off(declaration + 1);
    if !rewriter.rewrite_list(&mut tail) {
        return None;
    }
    *counter += 1;
    statements.push(rewriter.flag_reset(span));
    // Seed the detached local from the alias so it is definitely assigned on every path (a
    // maybe-unassigned read would widen its type with null and reject `++`/`--`). `global $x`
    // creates the global as null when it does not exist yet, so this read never warns.
    statements.push(rewriter.seed(span));
    statements.extend(tail);
    Some(statements)
}

/// Rewrites every use of one detached alias inside one function-like body.
struct ConditionalDetach {
    /// The aliased global's name, without the leading `$`.
    name: String,
    /// Hidden local holding the value captured when the global is detached.
    hidden: String,
    /// Hidden local recording whether the global was detached.
    flag: String,
}

impl ConditionalDetach {
    /// Rewrites one statement list, expanding a target unset into capture, unset, and flag set.
    fn rewrite_list(&self, list: &mut Vec<Stmt>) -> bool {
        let original = std::mem::take(list);
        let mut rewritten = Vec::with_capacity(original.len());
        for mut stmt in original {
            if self.is_target_unset(&stmt) {
                let span = stmt.span;
                rewritten.push(self.capture(span));
                rewritten.push(stmt);
                rewritten.push(self.mark_detached(span));
                continue;
            }
            if !self.rewrite_stmt(&mut stmt, true) {
                return false;
            }
            rewritten.push(stmt);
        }
        *list = rewritten;
        true
    }

    /// Rewrites one statement. `allow_branch` is false where the grammar has room for a single
    /// statement (a `for` init/update slot), so a write to the alias cannot become an `if`.
    fn rewrite_stmt(&self, stmt: &mut Stmt, allow_branch: bool) -> bool {
        // A target unset expands into three statements, so it is only handled from a statement
        // list. In a single-statement slot (`for` init/update) the shape cannot be expressed.
        if self.is_target_unset(stmt) {
            return false;
        }
        let replacement = match &mut stmt.kind {
            StmtKind::ExprStmt(expr) if self.is_alias_unset_call(expr) => {
                if !allow_branch {
                    return false;
                }
                let span = stmt.span;
                Some(self.branch(
                    span,
                    self.unset_call(span, &self.hidden),
                    self.unset_call(span, &self.name),
                ))
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::Include { path: expr, .. } => {
                if !self.rewrite_expr(expr) {
                    return false;
                }
                None
            }
            StmtKind::Return(Some(expr)) => {
                if !self.rewrite_expr(expr) {
                    return false;
                }
                None
            }
            StmtKind::Assign { name, value } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                if name == &self.name {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    let value = value.clone();
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::Assign {
                                name: self.hidden.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::Assign {
                                name: name.clone(),
                                value,
                            },
                            span,
                        ),
                    ))
                } else {
                    None
                }
            }
            StmtKind::TypedAssign {
                type_expr,
                name,
                value,
            } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                if name == &self.name {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    let value = value.clone();
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::Assign {
                                name: self.hidden.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::TypedAssign {
                                type_expr: type_expr.clone(),
                                name: name.clone(),
                                value,
                            },
                            span,
                        ),
                    ))
                } else {
                    None
                }
            }
            StmtKind::ArrayPush { array, value } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                if array == &self.name {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    let value = value.clone();
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::ArrayPush {
                                array: self.hidden.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(StmtKind::ArrayPush { array: array.clone(), value }, span),
                    ))
                } else {
                    None
                }
            }
            StmtKind::ArrayAssign { array, index, value } => {
                if !self.rewrite_expr(index) || !self.rewrite_expr(value) {
                    return false;
                }
                if array == &self.name {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::ArrayAssign {
                                array: self.hidden.clone(),
                                index: index.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::ArrayAssign {
                                array: array.clone(),
                                index: index.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                    ))
                } else {
                    None
                }
            }
            StmtKind::NestedArrayAssign { target, value } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                if let Some(detached) = self.retarget(target) {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::NestedArrayAssign {
                                target: detached,
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::NestedArrayAssign {
                                target: target.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                    ))
                } else if self.lvalue_mentions(target) {
                    return false;
                } else {
                    None
                }
            }
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                if let Some(detached) = self.retarget(object) {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::PropertyAssign {
                                object: Box::new(detached),
                                property: property.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::PropertyAssign {
                                object: object.clone(),
                                property: property.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                    ))
                } else if self.lvalue_mentions(object) {
                    return false;
                } else {
                    None
                }
            }
            StmtKind::PropertyArrayPush {
                object,
                property,
                value,
            } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                if let Some(detached) = self.retarget(object) {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::PropertyArrayPush {
                                object: Box::new(detached),
                                property: property.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::PropertyArrayPush {
                                object: object.clone(),
                                property: property.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                    ))
                } else if self.lvalue_mentions(object) {
                    return false;
                } else {
                    None
                }
            }
            StmtKind::PropertyArrayAssign {
                object,
                property,
                index,
                value,
            } => {
                if !self.rewrite_expr(index) || !self.rewrite_expr(value) {
                    return false;
                }
                if let Some(detached) = self.retarget(object) {
                    if !allow_branch {
                        return false;
                    }
                    let span = stmt.span;
                    Some(self.branch(
                        span,
                        Stmt::new(
                            StmtKind::PropertyArrayAssign {
                                object: Box::new(detached),
                                property: property.clone(),
                                index: index.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                        Stmt::new(
                            StmtKind::PropertyArrayAssign {
                                object: object.clone(),
                                property: property.clone(),
                                index: index.clone(),
                                value: value.clone(),
                            },
                            span,
                        ),
                    ))
                } else if self.lvalue_mentions(object) {
                    return false;
                } else {
                    None
                }
            }
            StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. }
            | StmtKind::ConstDecl { value, .. }
            | StmtKind::StaticVar { init: value, .. } => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                None
            }
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                if !self.rewrite_expr(index) || !self.rewrite_expr(value) {
                    return false;
                }
                None
            }
            StmtKind::ListUnpack { vars, value } => {
                if vars.iter().any(|var| var == &self.name) {
                    return false;
                }
                if !self.rewrite_expr(value) {
                    return false;
                }
                None
            }
            StmtKind::RefAssign { target, source } => {
                if target == &self.name || self.lvalue_mentions(source) {
                    return false;
                }
                if !self.rewrite_expr(source) {
                    return false;
                }
                None
            }
            StmtKind::Global { vars } => {
                if vars.iter().any(|var| var == &self.name) {
                    return false;
                }
                None
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                if !self.rewrite_expr(condition) || !self.rewrite_list(then_body) {
                    return false;
                }
                for (condition, body) in elseif_clauses {
                    if !self.rewrite_expr(condition) || !self.rewrite_list(body) {
                        return false;
                    }
                }
                if let Some(body) = else_body {
                    if !self.rewrite_list(body) {
                        return false;
                    }
                }
                None
            }
            StmtKind::IfDef {
                then_body, else_body, ..
            } => {
                if !self.rewrite_list(then_body) {
                    return false;
                }
                if let Some(body) = else_body {
                    if !self.rewrite_list(body) {
                        return false;
                    }
                }
                None
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
                if !self.rewrite_expr(condition) || !self.rewrite_list(body) {
                    return false;
                }
                None
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    if !self.rewrite_stmt(init, false) {
                        return false;
                    }
                }
                if let Some(condition) = condition {
                    if !self.rewrite_expr(condition) {
                        return false;
                    }
                }
                if let Some(update) = update {
                    if !self.rewrite_stmt(update, false) {
                        return false;
                    }
                }
                if !self.rewrite_list(body) {
                    return false;
                }
                None
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
                ..
            } => {
                if key_var.as_deref() == Some(self.name.as_str())
                    || value_var == &self.name
                {
                    return false;
                }
                if !self.rewrite_expr(array) || !self.rewrite_list(body) {
                    return false;
                }
                None
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                if !self.rewrite_expr(subject) {
                    return false;
                }
                for (labels, body) in cases {
                    for label in labels {
                        if !self.rewrite_expr(label) {
                            return false;
                        }
                    }
                    if !self.rewrite_list(body) {
                        return false;
                    }
                }
                if let Some(body) = default {
                    if !self.rewrite_list(body) {
                        return false;
                    }
                }
                None
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                if !self.rewrite_list(try_body) {
                    return false;
                }
                for catch in catches {
                    if catch.variable.as_deref() == Some(self.name.as_str()) {
                        return false;
                    }
                    if !self.rewrite_list(&mut catch.body) {
                        return false;
                    }
                }
                if let Some(body) = finally_body {
                    if !self.rewrite_list(body) {
                        return false;
                    }
                }
                None
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::Synthetic(body) => {
                if !self.rewrite_list(body) {
                    return false;
                }
                None
            }
            StmtKind::Return(None)
            | StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::IncludeOnceMark { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            // `goto` lowering runs before this pass, so no jump statement reaches it.
            | StmtKind::Goto(_)
            | StmtKind::GotoLabel(_) => None,
            // Nested declarations are separate scopes: a named function or a class method cannot
            // capture the alias, and its own `global`/unset pair is rewritten by the caller when it
            // reaches that body.
            StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. } => None,
        };
        if let Some(replacement) = replacement {
            *stmt = replacement;
        }
        true
    }

    /// Rewrites one expression, replacing a read of the alias with the conditional value.
    fn rewrite_expr(&self, expr: &mut Expr) -> bool {
        let replacement = match &mut expr.kind {
            ExprKind::Variable(variable) if variable == &self.name => Some(self.read(expr.span)),
            ExprKind::PreIncrement(variable) if variable == &self.name => {
                let then = Expr::new(ExprKind::PreIncrement(self.hidden.clone()), expr.span);
                let otherwise =
                    Expr::new(ExprKind::PreIncrement(self.name.clone()), expr.span);
                Some(self.choose(expr.span, then, otherwise))
            }
            ExprKind::PostIncrement(variable) if variable == &self.name => {
                let then = Expr::new(ExprKind::PostIncrement(self.hidden.clone()), expr.span);
                let otherwise =
                    Expr::new(ExprKind::PostIncrement(self.name.clone()), expr.span);
                Some(self.choose(expr.span, then, otherwise))
            }
            ExprKind::PreDecrement(variable) if variable == &self.name => {
                let then = Expr::new(ExprKind::PreDecrement(self.hidden.clone()), expr.span);
                let otherwise =
                    Expr::new(ExprKind::PreDecrement(self.name.clone()), expr.span);
                Some(self.choose(expr.span, then, otherwise))
            }
            ExprKind::PostDecrement(variable) if variable == &self.name => {
                let then = Expr::new(ExprKind::PostDecrement(self.hidden.clone()), expr.span);
                let otherwise =
                    Expr::new(ExprKind::PostDecrement(self.name.clone()), expr.span);
                Some(self.choose(expr.span, then, otherwise))
            }
            ExprKind::FunctionCall { name, args } if self.is_isset_of_alias(name, args) => {
                let then = self.isset_call(expr.span, &self.hidden);
                let otherwise = self.isset_call(expr.span, &self.name);
                Some(self.choose(expr.span, then, otherwise))
            }
            ExprKind::FunctionCall { name, args } if self.is_alias_unset_call_of(name, args) => {
                // `unset()` takes an lvalue, so it never becomes a conditional operand. The
                // statement form is rewritten by `rewrite_stmt`; anywhere else the shape cannot be
                // expressed.
                return false;
            }
            ExprKind::Closure {
                captures,
                capture_refs,
                ..
            } => {
                if captures
                    .iter()
                    .chain(capture_refs.iter())
                    .any(|capture| capture == &self.name)
                {
                    return false;
                }
                None
            }
            ExprKind::Assignment {
                target,
                value,
                prelude,
                result_target,
                ..
            } => {
                if self.lvalue_mentions(target)
                    || result_target
                        .as_deref()
                        .is_some_and(|target| self.lvalue_mentions(target))
                {
                    return false;
                }
                if !self.rewrite_list(prelude) || !self.rewrite_expr(value) {
                    return false;
                }
                None
            }
            ExprKind::BinaryOp { left, right, .. } => {
                if !self.rewrite_expr(left) || !self.rewrite_expr(right) {
                    return false;
                }
                None
            }
            ExprKind::Reference(inner) => {
                if self.lvalue_mentions(inner) || !self.rewrite_expr(inner) {
                    return false;
                }
                None
            }
            ExprKind::InstanceOf { value, .. }
            | ExprKind::Negate(value)
            | ExprKind::Not(value)
            | ExprKind::BitNot(value)
            | ExprKind::Throw(value)
            | ExprKind::ErrorSuppress(value)
            | ExprKind::Print(value)
            | ExprKind::Cast { expr: value, .. }
            | ExprKind::NamedArg { value, .. }
            | ExprKind::IncludeValue { path: value, .. }
            | ExprKind::Spread(value)
            | ExprKind::Clone(value)
            | ExprKind::PropertyAccess { object: value, .. }
            | ExprKind::NullsafePropertyAccess { object: value, .. }
            | ExprKind::PtrCast { expr: value, .. }
            | ExprKind::BufferNew { len: value, .. }
            | ExprKind::ObjectClassName { object: value }
            | ExprKind::YieldFrom(value) => {
                if !self.rewrite_expr(value) {
                    return false;
                }
                None
            }
            ExprKind::NullCoalesce { value, default }
            | ExprKind::ShortTernary { value, default } => {
                if !self.rewrite_expr(value) || !self.rewrite_expr(default) {
                    return false;
                }
                None
            }
            ExprKind::Pipe { value, callable } => {
                if !self.rewrite_expr(value) || !self.rewrite_expr(callable) {
                    return false;
                }
                None
            }
            ExprKind::ArrayAccess { array, index } => {
                if !self.rewrite_expr(array) || !self.rewrite_expr(index) {
                    return false;
                }
                None
            }
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                if !self.rewrite_expr(object) || !self.rewrite_expr(property) {
                    return false;
                }
                None
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                if !self.rewrite_expr(condition)
                    || !self.rewrite_expr(then_expr)
                    || !self.rewrite_expr(else_expr)
                {
                    return false;
                }
                None
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                if !self.rewrite_expr(subject) {
                    return false;
                }
                for (conditions, result) in arms {
                    for condition in conditions {
                        if !self.rewrite_expr(condition) {
                            return false;
                        }
                    }
                    if !self.rewrite_expr(result) {
                        return false;
                    }
                }
                if let Some(default) = default {
                    if !self.rewrite_expr(default) {
                        return false;
                    }
                }
                None
            }
            ExprKind::FunctionCall { args, .. }
            | ExprKind::ClosureCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewScopedObject { args, .. }
            | ExprKind::ArrayLiteral(args) => {
                for arg in args {
                    if !self.rewrite_expr(arg) {
                        return false;
                    }
                }
                None
            }
            ExprKind::ArrayLiteralAssoc(entries) => {
                for (key, value) in entries {
                    if !self.rewrite_expr(key) || !self.rewrite_expr(value) {
                        return false;
                    }
                }
                None
            }
            ExprKind::ArrayLiteralMixed(entries) => {
                for entry in entries {
                    for operand in entry.exprs_mut() {
                        if !self.rewrite_expr(operand) {
                            return false;
                        }
                    }
                }
                None
            }
            ExprKind::ExprCall { callee, args } => {
                if !self.rewrite_expr(callee) {
                    return false;
                }
                for arg in args {
                    if !self.rewrite_expr(arg) {
                        return false;
                    }
                }
                None
            }
            ExprKind::NewDynamic {
                name_expr: receiver,
                args,
            }
            | ExprKind::NewDynamicObject {
                class_name: receiver,
                args,
                ..
            }
            | ExprKind::MethodCall {
                object: receiver,
                args,
                ..
            }
            | ExprKind::NullsafeMethodCall {
                object: receiver,
                args,
                ..
            } => {
                if !self.rewrite_expr(receiver) {
                    return false;
                }
                for arg in args {
                    if !self.rewrite_expr(arg) {
                        return false;
                    }
                }
                None
            }
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => {
                if !self.rewrite_expr(object) || !self.rewrite_expr(method) {
                    return false;
                }
                for arg in args {
                    if !self.rewrite_expr(arg) {
                        return false;
                    }
                }
                None
            }
            ExprKind::Yield { key, value } => {
                if let Some(key) = key {
                    if !self.rewrite_expr(key) {
                        return false;
                    }
                }
                if let Some(value) = value {
                    if !self.rewrite_expr(value) {
                        return false;
                    }
                }
                None
            }
            ExprKind::NewGeneric { args, .. } => {
                for arg in args {
                    if !self.rewrite_expr(arg) {
                        return false;
                    }
                }
                None
            }
            ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::Variable(_)
            | ExprKind::PreIncrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::ConstRef(_)
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::FirstClassCallable(_)
            | ExprKind::This
            | ExprKind::ClassConstant { .. }
            | ExprKind::ScopedConstantAccess { .. }
            | ExprKind::MagicConstant(_)
            | ExprKind::Runtime(_) => None,
        };
        if let Some(replacement) = replacement {
            *expr = replacement;
        }
        true
    }

    /// Builds `$flag ? $hidden : $x`.
    fn read(&self, span: Span) -> Expr {
        let hidden = Expr::new(ExprKind::Variable(self.hidden.clone()), span);
        let otherwise = Expr::new(ExprKind::Variable(self.name.clone()), span);
        self.choose(span, hidden, otherwise)
    }

    /// Builds `$flag ? $then : $otherwise`.
    fn choose(&self, span: Span, then_expr: Expr, else_expr: Expr) -> Expr {
        Expr::new(
            ExprKind::Ternary {
                condition: Box::new(self.flag_read(span)),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
            span,
        )
    }

    /// Builds the `if ($flag) { … } else { … }` statement a write needs.
    fn branch(&self, span: Span, then_stmt: Stmt, else_stmt: Stmt) -> Stmt {
        Stmt::new(
            StmtKind::If {
                condition: self.flag_read(span),
                then_body: vec![then_stmt],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![else_stmt]),
            },
            span,
        )
    }

    /// Builds `$flag = false;`.
    fn flag_reset(&self, span: Span) -> Stmt {
        Stmt::new(
            StmtKind::Assign {
                name: self.flag.clone(),
                value: Expr::new(ExprKind::BoolLiteral(false), span),
            },
            span,
        )
    }

    /// Builds the capture that keeps the removed value reachable through the local.
    ///
    /// The capture is guarded by the flag rather than reading the effective value, so `$hidden` is
    /// only ever assigned from the alias and keeps its type. It is also the correct rule for a
    /// second detachment: once the alias is detached, `$x` names the (possibly recreated) global
    /// slot, and PHP would not re-capture it. The guard builds `if (!$flag) { $hidden = $x; }`.
    fn capture(&self, span: Span) -> Stmt {
        Stmt::new(
            StmtKind::If {
                condition: Expr::new(
                    ExprKind::Not(Box::new(self.flag_read(span))),
                    span,
                ),
                then_body: vec![Stmt::new(
                    StmtKind::Assign {
                        name: self.hidden.clone(),
                        value: self.variable(span, &self.name),
                    },
                    span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        )
    }

    /// Builds the initial `$hidden = $x;` seed described by `detach`.
    fn seed(&self, span: Span) -> Stmt {
        Stmt::new(
            StmtKind::Assign {
                name: self.hidden.clone(),
                value: self.variable(span, &self.name),
            },
            span,
        )
    }

    /// Builds `$flag = true;`.
    fn mark_detached(&self, span: Span) -> Stmt {
        Stmt::new(
            StmtKind::Assign {
                name: self.flag.clone(),
                value: Expr::new(ExprKind::BoolLiteral(true), span),
            },
            span,
        )
    }

    /// Builds `$flag` as a read.
    fn flag_read(&self, span: Span) -> Expr {
        Expr::new(ExprKind::Variable(self.flag.clone()), span)
    }

    /// Builds `$name` as a read.
    fn variable(&self, span: Span, name: &str) -> Expr {
        Expr::new(ExprKind::Variable(name.to_string()), span)
    }

    /// Builds an `isset($name)` call.
    fn isset_call(&self, span: Span, name: &str) -> Expr {
        Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::unqualified("isset"),
                args: vec![self.variable(span, name)],
            },
            span,
        )
    }

    /// Builds an `unset($name)` call.
    fn unset_call(&self, span: Span, name: &str) -> Stmt {
        Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: crate::names::Name::unqualified("unset"),
                    args: vec![self.variable(span, name)],
                },
                span,
            )),
            span,
        )
    }

    /// Reports whether `expr` is exactly `unset($GLOBALS[$name])`.
    fn is_target_unset(&self, stmt: &Stmt) -> bool {
        matches!(&stmt.kind, StmtKind::ExprStmt(expr) if self.is_globals_unset(expr))
    }

    /// Reports whether `expr` unsets this alias through the `$GLOBALS` literal form.
    fn is_globals_unset(&self, expr: &Expr) -> bool {
        let ExprKind::FunctionCall { name, args } = &expr.kind else {
            return false;
        };
        if !name.as_str().eq_ignore_ascii_case("unset") || args.len() != 1 {
            return false;
        }
        let ExprKind::ArrayAccess { array, index } = &args[0].kind else {
            return false;
        };
        if !matches!(&array.kind, ExprKind::Variable(base) if base == "GLOBALS") {
            return false;
        }
        matches!(&index.kind, ExprKind::StringLiteral(key) if key == &self.name)
    }

    /// Reports whether `expr` is exactly `unset($x)` for this alias.
    fn is_alias_unset_call(&self, expr: &Expr) -> bool {
        let ExprKind::FunctionCall { name, args } = &expr.kind else {
            return false;
        };
        self.is_alias_unset_call_of(name, args)
    }

    /// Reports whether a call name and argument list are `unset($x)` for this alias.
    fn is_alias_unset_call_of(&self, call: &crate::names::Name, args: &[Expr]) -> bool {
        call.as_str().eq_ignore_ascii_case("unset")
            && args.len() == 1
            && matches!(&args[0].kind, ExprKind::Variable(variable) if variable == &self.name)
    }

    /// Reports whether a call name and argument list are `isset($x)` for this alias.
    fn is_isset_of_alias(&self, call: &crate::names::Name, args: &[Expr]) -> bool {
        call.as_str().eq_ignore_ascii_case("isset")
            && args.len() == 1
            && matches!(&args[0].kind, ExprKind::Variable(variable) if variable == &self.name)
    }

    /// Reports whether an lvalue chain is rooted at the alias.
    fn lvalue_mentions(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Variable(variable) => variable == &self.name,
            ExprKind::ArrayAccess { array, .. } => self.lvalue_mentions(array),
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::DynamicPropertyAccess { object, .. } => self.lvalue_mentions(object),
            _ => false,
        }
    }

    /// Rebuilds an lvalue chain with the alias root swapped for the detached copy.
    ///
    /// Returns `None` when the chain is not rooted at the alias, so a caller can fall back to a
    /// plain read rewrite.
    fn retarget(&self, expr: &Expr) -> Option<Expr> {
        match &expr.kind {
            ExprKind::Variable(variable) if variable == &self.name => Some(Expr::new(
                ExprKind::Variable(self.hidden.clone()),
                expr.span,
            )),
            ExprKind::ArrayAccess { array, index } => {
                let array = self.retarget(array)?;
                Some(Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(array),
                        index: index.clone(),
                    },
                    expr.span,
                ))
            }
            ExprKind::PropertyAccess { object, property } => {
                let object = self.retarget(object)?;
                Some(Expr::new(
                    ExprKind::PropertyAccess {
                        object: Box::new(object),
                        property: property.clone(),
                    },
                    expr.span,
                ))
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                let object = self.retarget(object)?;
                Some(Expr::new(
                    ExprKind::DynamicPropertyAccess {
                        object: Box::new(object),
                        property: property.clone(),
                    },
                    expr.span,
                ))
            }
            _ => None,
        }
    }
}
