//! Purpose:
//! Lowers `include`/`require` used as an expression anywhere in a statement — an argument
//! (`array_replace_recursive(require __DIR__.'/en.php', [...])`, Carbon's lang files), an
//! operand (`10 + require F`), an array element, a static-property value, a `??=` value or a
//! ternary branch — into the one shape the resolver can expand: `$tmp = include F;` followed by
//! the statement reading `$tmp`.
//!
//! Called from:
//! - `crate::resolver::engine::resolve_stmts()`, before generic expression resolution.
//!
//! Key details:
//! - The included file runs in the CALLER's scope, so it is expanded as statements, not as an
//!   expression, in front of the statement. To keep PHP's evaluation order, every operand PHP
//!   has finished evaluating when the include runs is materialized into a temporary in front of
//!   it, in order: `f(g(), $o->m(), require F)` is `$a = g(); $b = $o->m(); $c = include F;
//!   f($a, $b, $c)`. That covers values PHP copies before the file runs (an array element, a
//!   static property, a dimension, a method receiver, a dynamic callee), so a file that writes
//!   them cannot change what the statement uses. A variable operand of a binary operator is NOT
//!   snapshotted: php-src fetches that CV when the operator runs, after the include. Two passes
//!   over the same walk decide this: a dry run finds the operands that precede the last
//!   include, the real run snapshots exactly those.
//! - A variable or property passed as a call argument before an include of the same call is a
//!   compile error: the parameter may be by reference, which a copy would break, and the
//!   resolver does not know the callee.
//! - An include in a conditionally evaluated operand (a ternary branch, the right side of `??`,
//!   `?:`, `&&`, `||`, so also `$x ??= require F`) hoists the whole conditional into an `if`
//!   that runs the include only when PHP would, with the operator left in place over one
//!   temporary per operand (see `Hoister::hoist_conditional`). A temporary assigned in one
//!   branch only is typed `mixed` by the checker, so a typed `return` of such a value can be
//!   refused at compile time; it is never silently wrong.
//! - An include in a `match` arm, a nullsafe call's arguments, an assignment expression or
//!   another node this walk does not model is a compile error.
//! - Closure and arrow-function bodies are not entered: their includes are deferred, and the
//!   closure body is resolved as its own statement list.
//! - Anything left over is refused by the safety net in `crate::resolver::exprs::resolve_expr`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::errors::CompileError;
use crate::parser::ast::{ArrayEntry, BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::contains::expr_has_includes;

/// Process-global counter producing unique hidden temporaries for hoisted includes.
pub(super) static HOIST_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Rewrites the expression-position includes in `stmt`'s own expressions, unless the include
/// is already the whole value of a plain assignment or a `return` (those are expanded directly
/// by `engine::try_expand_value_include`). Returns `None` when there is nothing to rewrite.
///
/// On success it returns the hoisted `$tmp = include F;` statements, to run first, and the
/// rewritten statement, which reads the temporaries and keeps any conditionally evaluated
/// include as a statement expression.
///
/// A `static $x;` local assigned a conditional hoist (`static $p; $p ??= require F;`) needs no
/// special case: an uninformative static initializer gives the local boxed `Mixed` storage (see
/// `static_local_declared_type`), so the `mixed` value the hoist leaves persists across calls and
/// the include runs only while the static is still null.
pub(super) fn hoist_value_includes(
    stmt: &Stmt,
) -> Result<Option<(Vec<Stmt>, Stmt)>, CompileError> {
    if !stmt_head_has_includes(stmt) {
        return Ok(None);
    }
    // Two passes over the same deterministic walk. The dry run on a copy finds, in evaluation
    // order, which operand reads come before the last hoisted include; the real run snapshots
    // exactly those into temporaries in front of it (see `Hoister::read_event`).
    let mut dry_run = stmt.clone();
    let mut dry = Hoister {
        dry: true,
        ..Hoister::default()
    };
    if !dry.walk_stmt(&mut dry_run.kind, stmt.span)? {
        return Ok(None);
    }
    let last_include = dry.include_events.last().copied().unwrap_or(0);
    let snapshot = dry
        .read_events
        .iter()
        .copied()
        .filter(|&read| read < last_include)
        .collect();
    let mut rewritten = stmt.clone();
    let mut hoister = Hoister {
        snapshot,
        ..Hoister::default()
    };
    hoister.walk_stmt(&mut rewritten.kind, stmt.span)?;
    if hoister.hoisted.is_empty() {
        return Ok(None);
    }
    Ok(Some((hoister.hoisted, rewritten)))
}

/// Whether the statement's own (non-body) expressions hold an include. A cheap pre-check so
/// statements without one are not cloned.
fn stmt_head_has_includes(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // The direct forms are expanded by `engine::try_expand_value_include`.
        StmtKind::Assign { value, .. } | StmtKind::Return(Some(value))
            if matches!(value.kind, ExprKind::IncludeValue { .. }) =>
        {
            false
        }
        StmtKind::ExprStmt(value)
        | StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::Return(Some(value))
        | StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::If { condition: value, .. }
        | StmtKind::Switch { subject: value, .. }
        | StmtKind::Foreach { array: value, .. } => expr_has_includes(value),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_has_includes(index) || expr_has_includes(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_has_includes(target) || expr_has_includes(value)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_has_includes(object) || expr_has_includes(value)
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            expr_has_includes(object) || expr_has_includes(index) || expr_has_includes(value)
        }
        _ => false,
    }
}

/// Collects the hoisted statements while walking one statement in evaluation order.
#[derive(Default)]
struct Hoister {
    /// Statements to run before the rewritten statement, in order.
    hoisted: Vec<Stmt>,
    /// Whether a conditional (`?:`, `??`, `&&`...) was hoisted into an `if`.
    hoisted_conditional: bool,
    /// The dry run only records events; it rewrites a throwaway copy.
    dry: bool,
    /// Evaluation-order counter shared by operand reads and includes.
    events: usize,
    /// Dry run: the event index of every operand read that may need a snapshot.
    read_events: Vec<usize>,
    /// Dry run: the event index of every hoisted include (or conditional holding one).
    include_events: Vec<usize>,
    /// Real run: the operand reads to snapshot, because an include is hoisted after them.
    snapshot: HashSet<usize>,
}

/// How the operand being visited is read by the operation that consumes it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// Copied when evaluated: an array element, a method receiver, a dynamic callee, a
    /// condition. A variable here is captured at its place in evaluation order.
    Eager,
    /// A variable operand the consuming opcode fetches only when it runs (php-src reads a CV
    /// operand of `ADD`, `CONCAT`, `FETCH_DIM_R`, `ASSIGN_DIM`... at execution), which is after
    /// anything hoisted, as in PHP.
    Lazy,
    /// A call argument, which may be passed by reference: it cannot be copied.
    Arg,
}

impl Hoister {
    /// Walks the statement's own expressions in evaluation order. Returns `false` for a
    /// statement kind this module does not lower.
    fn walk_stmt(&mut self, kind: &mut StmtKind, span: Span) -> Result<bool, CompileError> {
        match kind {
            StmtKind::ExprStmt(value)
            | StmtKind::Echo(value)
            | StmtKind::Throw(value)
            | StmtKind::Return(Some(value))
            | StmtKind::ListUnpack { value, .. } => self.visit(value)?,
            StmtKind::Assign { value, .. }
            | StmtKind::TypedAssign { value, .. }
            | StmtKind::ArrayPush { value, .. }
            | StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => self.visit_assigned(value, span)?,
            // `ASSIGN_DIM`/`ASSIGN_OBJ` fetch a variable container and a variable dimension when
            // they run, after the value; a computed dimension is evaluated first.
            StmtKind::ArrayAssign { index, value, .. }
            | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                self.visit_in(index, Ctx::Lazy)?;
                self.visit_assigned(value, span)?;
            }
            StmtKind::NestedArrayAssign { target, value } => {
                self.visit_target(target)?;
                self.visit_assigned(value, span)?;
            }
            StmtKind::PropertyAssign { object, value, .. }
            | StmtKind::PropertyArrayPush { object, value, .. } => {
                self.visit_target(object)?;
                self.visit_assigned(value, span)?;
            }
            StmtKind::PropertyArrayAssign { object, index, value, .. } => {
                self.visit_target(object)?;
                self.visit_in(index, Ctx::Lazy)?;
                self.visit_assigned(value, span)?;
            }
            // Only the expression evaluated once, before any body, can be hoisted. An `elseif`
            // condition, a `case` value or a loop condition runs conditionally or repeatedly and
            // is left to the safety net.
            StmtKind::If { condition, .. } => self.visit(condition)?,
            StmtKind::Switch { subject, .. } => self.visit(subject)?,
            StmtKind::Foreach { array, .. } => self.visit(array)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Visits an assignment target (`$a[$i][f()]`, `$o->p`): it is written, not read, so only
    /// the dimensions it computes are walked; php-src delays the container fetches until after
    /// the value (`zend_delayed_compile_dim`).
    fn visit_target(&mut self, target: &mut Expr) -> Result<(), CompileError> {
        match &mut target.kind {
            ExprKind::ArrayAccess { array, index } => {
                self.visit_target(array)?;
                self.visit_in(index, Ctx::Lazy)
            }
            ExprKind::PropertyAccess { object, .. } => self.visit_target(object),
            ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => Ok(()),
            _ => self.visit_in(target, Ctx::Lazy),
        }
    }

    /// Returns the next evaluation-order event index.
    fn next_event(&mut self) -> usize {
        self.events += 1;
        self.events
    }

    /// Visits the value of an assignment statement.
    ///
    /// A compound assignment (`K::$a[0] .= require F`) arrives as `<target> <op> <rhs>`, with a
    /// copy of the target read on the left. php-src's `ASSIGN_*_OP` evaluates the right operand
    /// BEFORE it reads the target, so only the right operand is walked. The parser gives that
    /// synthesized operator the STATEMENT's span (`assign::compound::assignment_value`), while a
    /// written operator's span starts at its operator token, after the `=`; so an explicit
    /// `$a[0] = $a[0] + require F`, whose left operand comes after the `=`, is an ordinary
    /// binary operator, and its `$a[0]` is read (and snapshotted) before the include, as in PHP.
    fn visit_assigned(&mut self, value: &mut Expr, stmt_span: Span) -> Result<(), CompileError> {
        let starts_with_stmt =
            value.span.line == stmt_span.line && value.span.col == stmt_span.col;
        if let ExprKind::BinaryOp { left, op, right } = &mut value.kind {
            let is_compound_copy = starts_with_stmt && is_place_read(left);
            if !matches!(op, BinOp::And | BinOp::Or | BinOp::NullCoalesce) && is_compound_copy {
                return self.visit_in(right, Ctx::Lazy);
            }
        }
        self.visit(value)
    }

    /// Visits `expr` in PHP evaluation order, replacing each hoistable include (or conditional
    /// holding one) by a hidden temporary.
    fn visit(&mut self, expr: &mut Expr) -> Result<(), CompileError> {
        self.visit_in(expr, Ctx::Eager)
    }

    /// [`Hoister::visit`] for an operand consumed as `ctx` says.
    ///
    /// Every operand that PHP has finished evaluating when a later include of the statement runs
    /// is materialized into a temporary in front of that include (see `completed`), so the
    /// hoisted form keeps PHP's evaluation order exactly: `f(g(), require F, h())` becomes
    /// `$a = g(); $b = include F; f($a, $b, h())`. A node without an include is one such unit;
    /// a node holding one is walked, then is itself a unit once its operands are done.
    fn visit_in(&mut self, expr: &mut Expr, ctx: Ctx) -> Result<(), CompileError> {
        if is_inert_leaf(expr) || (ctx == Ctx::Lazy && matches!(expr.kind, ExprKind::Variable(_))) {
            return Ok(());
        }
        // A named argument is a label on its value, not an operand of its own.
        if let ExprKind::NamedArg { value, .. } = &mut expr.kind {
            return self.visit_in(value, ctx);
        }
        if !expr_has_includes(expr) {
            return self.completed(expr, ctx);
        }
        self.descend(expr)?;
        if matches!(expr.kind, ExprKind::Variable(_)) {
            // An include (or hoisted conditional) replaced by its temporary.
            return Ok(());
        }
        self.completed(expr, ctx)
    }

    /// Records that `expr` has been fully evaluated at this point of the evaluation order and,
    /// when an include is hoisted after it, snapshots its value into a temporary in front of
    /// that include, which is where PHP evaluates it.
    ///
    /// A variable or property passed as a call argument cannot be snapshotted: the parameter
    /// may be by reference (the resolver does not know the callee), so `f($x, require F)` is a
    /// compile error rather than a copy. A spread argument cannot be snapshotted either.
    fn completed(&mut self, expr: &mut Expr, ctx: Ctx) -> Result<(), CompileError> {
        let event = self.next_event();
        if self.dry {
            self.read_events.push(event);
            return Ok(());
        }
        if !self.snapshot.contains(&event) {
            return Ok(());
        }
        let span = expr.span;
        if (ctx == Ctx::Arg && is_place_read(expr)) || matches!(expr.kind, ExprKind::Spread(_)) {
            return Err(CompileError::new(
                span,
                "a variable, property or spread passed to a call before an include/require of \
                 the same call is not supported yet: the include could change it and the \
                 argument may be by reference; assign the include to a local variable first",
            ));
        }
        let temp = fresh_temp();
        let value = std::mem::replace(expr, Expr::new(ExprKind::Variable(temp.clone()), span));
        self.hoisted.push(assign(&temp, value, span));
        Ok(())
    }

    /// Walks the operands of a node that holds an include, in PHP evaluation order, each with
    /// the context its consumer reads it in.
    fn descend(&mut self, expr: &mut Expr) -> Result<(), CompileError> {
        let span = expr.span;
        // `return self::$cache[$k] ??= require F;`: a non-local `??=` whose lazy value holds
        // the include. See `hoist_coalesce_assignment`.
        if coalesce_assignment_include_parts(expr).is_some() {
            return self.hoist_coalesce_assignment(expr);
        }
        match &mut expr.kind {
            ExprKind::IncludeValue { .. } => {
                let event = self.next_event();
                if self.dry {
                    self.include_events.push(event);
                }
                let temp = fresh_temp();
                let include =
                    std::mem::replace(expr, Expr::new(ExprKind::Variable(temp.clone()), span));
                self.hoisted.push(assign(&temp, include, span));
                Ok(())
            }

            // Containers: elements are added as they are evaluated.
            ExprKind::ArrayLiteral(items) => {
                for item in items {
                    self.visit(item)?;
                }
                Ok(())
            }
            ExprKind::ArrayLiteralAssoc(entries) => {
                for (key, value) in entries {
                    self.visit_entry(key, value)?;
                }
                Ok(())
            }
            ExprKind::ArrayLiteralMixed(entries) => {
                for entry in entries {
                    match entry {
                        ArrayEntry::Keyed(key, value) => self.visit_entry(key, value)?,
                        ArrayEntry::Spread(value) | ArrayEntry::Value(value) => self.visit(value)?,
                    }
                }
                Ok(())
            }
            // `@include F`: the include is hoisted out of the `@`, as the statement form
            // `@include F;` already drops the operator.
            ExprKind::ErrorSuppress(inner) => self.visit(inner),

            // Conditionally evaluated operands: an include in the lazy operand moves into an
            // `if` with the rest of the conditional (see `hoist_conditional`). The first operand
            // is read right away (`JMPZ`, `COALESCE`, `JMP_SET` on a CV).
            ExprKind::BinaryOp {
                left,
                op: BinOp::And | BinOp::Or,
                right,
            } => {
                if expr_has_includes(right) {
                    return self.hoist_conditional(expr);
                }
                self.visit(left)
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                if expr_has_includes(then_expr) || expr_has_includes(else_expr) {
                    return self.hoist_conditional(expr);
                }
                self.visit(condition)
            }
            ExprKind::NullCoalesce { value, default }
            | ExprKind::ShortTernary { value, default } => {
                if expr_has_includes(default) {
                    return self.hoist_conditional(expr);
                }
                self.visit(value)
            }
            ExprKind::Match { subject, arms, default } => {
                let arms_have_include = arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_has_includes) || expr_has_includes(value)
                }) || default.as_deref().is_some_and(expr_has_includes);
                if arms_have_include {
                    return Err(CompileError::new(
                        span,
                        "include/require in a `match` arm is not supported yet; \
                         use an `if` and assign it to a local variable first",
                    ));
                }
                self.visit(subject)
            }

            // Operators and accesses fetch a variable operand when they run.
            ExprKind::BinaryOp { left, right, .. } => {
                self.visit_in(left, Ctx::Lazy)?;
                self.visit_in(right, Ctx::Lazy)
            }
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Print(inner)
            | ExprKind::Clone(inner)
            | ExprKind::Throw(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::InstanceOf { value: inner, .. }
            | ExprKind::PropertyAccess { object: inner, .. }
            | ExprKind::ObjectClassName { object: inner } => self.visit_in(inner, Ctx::Lazy),
            ExprKind::ArrayAccess { array, index } => {
                self.visit_in(array, Ctx::Lazy)?;
                self.visit_in(index, Ctx::Lazy)
            }
            ExprKind::Pipe { value, callable } => {
                self.visit(value)?;
                self.visit(callable)
            }

            // Calls: the receiver or callee is read first (`INIT_METHOD_CALL`,
            // `INIT_DYNAMIC_CALL`), then the arguments.
            ExprKind::FunctionCall { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::NewGeneric { args, .. }
            | ExprKind::NewScopedObject { args, .. } => self.visit_args(args),
            ExprKind::MethodCall { object, args, .. } => {
                self.visit(object)?;
                self.visit_args(args)
            }
            ExprKind::ExprCall { callee, args } => {
                self.visit(callee)?;
                self.visit_args(args)
            }
            ExprKind::NewDynamic { name_expr, args } => {
                self.visit(name_expr)?;
                self.visit_args(args)
            }
            ExprKind::ClosureCall { var, args } => {
                let mut callee = Expr::new(ExprKind::Variable(var.clone()), span);
                self.completed(&mut callee, Ctx::Eager)?;
                if let ExprKind::Variable(name) = callee.kind {
                    *var = name;
                }
                self.visit_args(args)
            }

            // Assignment expressions with their preludes, nullsafe chains whose arguments run
            // conditionally, `yield`, FFI nodes... are not modelled.
            _ => Err(CompileError::new(
                span,
                "include/require is not supported inside this expression yet; \
                 assign it to a local variable first",
            )),
        }
    }

    /// Visits one `key => value` element of an array literal.
    ///
    /// php-src compiles the key first, then the value, then `ADD_ARRAY_ELEMENT`, which fetches a
    /// CV key only when it runs (MEASURED: `[k(1) => require F]` calls `k` before the file, and
    /// `[$x => require F]` uses the `$x` the file wrote). So a computed key is an ordinary
    /// operand before the value, while a variable key is read AFTER the value, yet still before
    /// any later element: its read is recorded after the value's events, which snapshots it in
    /// front of a later include only.
    fn visit_entry(&mut self, key: &mut Expr, value: &mut Expr) -> Result<(), CompileError> {
        if matches!(key.kind, ExprKind::Variable(_)) {
            self.visit(value)?;
            return self.completed(key, Ctx::Eager);
        }
        self.visit(key)?;
        self.visit(value)
    }

    /// Visits call arguments in order.
    fn visit_args(&mut self, args: &mut [Expr]) -> Result<(), CompileError> {
        for arg in args {
            self.visit_in(arg, Ctx::Arg)?;
        }
        Ok(())
    }

    /// Hoists a conditional whose lazily evaluated operand holds an include into an `if` that
    /// runs that operand only when PHP would, and leaves the operator in place over
    /// temporaries:
    ///
    /// - `c ? a : b` → `$c = c; if ($c) { $a = a; } else { $b = b; }`, then `$c ? $a : $b`
    ///   (a branch without an include stays inline and is still evaluated lazily);
    /// - `v ?? d` → `$v = v ?? null; if ($v === null) { $d = d; }`, then `$v ?? $d`;
    /// - `v ?: d` → `$v = v; if (!$v) { $d = d; }`, then `$v ?: $d`;
    /// - `l && r` / `l || r` → `$l = l; if ($l) { $r = r; }` (`if (!$l)` for `||`), then
    ///   `$l && $r` / `$l || $r`.
    ///
    /// Every operand gets its OWN temporary and the operator stays: elephc types a local from
    /// its assignments, so one temporary shared by operands of different types would not
    /// type-check, while the operator unifies them as the original did. The first operand is
    /// evaluated once; operands before the conditional are materialized like before any include.
    fn hoist_conditional(&mut self, expr: &mut Expr) -> Result<(), CompileError> {
        let span = expr.span;
        self.hoisted_conditional = true;
        let event = self.next_event();
        if self.dry {
            self.include_events.push(event);
        }
        let placeholder = Expr::new(ExprKind::Null, span);
        let original = std::mem::replace(expr, placeholder).kind;
        let null = || Expr::new(ExprKind::Null, span);
        match original {
            ExprKind::Ternary {
                mut condition,
                then_expr,
                else_expr,
            } => {
                // Includes in the condition itself are hoisted first, in order.
                self.visit(&mut condition)?;
                let cond_temp = fresh_temp();
                self.hoisted.push(assign(&cond_temp, *condition, span));
                let (then_body, then_expr) = operand_into_temp(*then_expr, span);
                let (else_body, else_expr) = operand_into_temp(*else_expr, span);
                self.push_if(variable(&cond_temp, span), then_body, else_body, span);
                *expr = Expr::new(
                    ExprKind::Ternary {
                        condition: Box::new(variable(&cond_temp, span)),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    },
                    span,
                );
            }
            ExprKind::NullCoalesce { mut value, default } => {
                self.visit(&mut value)?;
                let value_temp = fresh_temp();
                let probe = ExprKind::NullCoalesce { value, default: Box::new(null()) };
                self.hoisted.push(assign(&value_temp, Expr::new(probe, span), span));
                let is_null = ExprKind::BinaryOp {
                    left: Box::new(variable(&value_temp, span)),
                    op: BinOp::StrictEq,
                    right: Box::new(null()),
                };
                let (body, default) = operand_into_temp(*default, span);
                self.push_if(Expr::new(is_null, span), body, Vec::new(), span);
                *expr = Expr::new(
                    ExprKind::NullCoalesce {
                        value: Box::new(variable(&value_temp, span)),
                        default: Box::new(default),
                    },
                    span,
                );
            }
            ExprKind::ShortTernary { mut value, default } => {
                self.visit(&mut value)?;
                let value_temp = fresh_temp();
                self.hoisted.push(assign(&value_temp, *value, span));
                let falsy = ExprKind::Not(Box::new(variable(&value_temp, span)));
                let (body, default) = operand_into_temp(*default, span);
                self.push_if(Expr::new(falsy, span), body, Vec::new(), span);
                *expr = Expr::new(
                    ExprKind::ShortTernary {
                        value: Box::new(variable(&value_temp, span)),
                        default: Box::new(default),
                    },
                    span,
                );
            }
            ExprKind::BinaryOp { mut left, op, right } => {
                self.visit(&mut left)?;
                let left_temp = fresh_temp();
                self.hoisted.push(assign(&left_temp, *left, span));
                let runs_right = if op == BinOp::And {
                    variable(&left_temp, span)
                } else {
                    Expr::new(ExprKind::Not(Box::new(variable(&left_temp, span))), span)
                };
                let (body, right) = operand_into_temp(*right, span);
                self.push_if(runs_right, body, Vec::new(), span);
                *expr = Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(variable(&left_temp, span)),
                        op,
                        right: Box::new(right),
                    },
                    span,
                );
            }
            _ => unreachable!("hoist_conditional is only called on a conditional operator"),
        }
        Ok(())
    }

    /// Hoists a non-local `place ??= <value holding an include>` used as an expression.
    ///
    /// The parser's prelude (which stabilizes the place's computed parts, in source order) runs
    /// first, then the STATEMENT form `place ??= value;`, which the resolver already lowers
    /// (its conditional include runs only when the place is unset or null), and the expression
    /// becomes a read of the place: PHP's `??=` yields the place's value whether it was already
    /// set or just written.
    fn hoist_coalesce_assignment(&mut self, expr: &mut Expr) -> Result<(), CompileError> {
        let span = expr.span;
        self.hoisted_conditional = true;
        let event = self.next_event();
        if self.dry {
            self.include_events.push(event);
        }
        let original = std::mem::replace(expr, Expr::new(ExprKind::Null, span));
        let ExprKind::Assignment { target, value, result_target, prelude, .. } = original.kind
        else {
            unreachable!("guarded by coalesce_assignment_include_parts");
        };
        let read = result_target.map(|target| *target).unwrap_or_else(|| (*target).clone());
        let store = crate::parser::stmt::assignment_target_store_stmt(*target, *value, span)?;
        self.hoisted.extend(prelude);
        self.hoisted.push(Stmt::new(store, span));
        *expr = read;
        Ok(())
    }

    /// Pushes `if (condition) { then_body } [else { else_body }]` onto the hoisted statements.
    fn push_if(&mut self, condition: Expr, then_body: Vec<Stmt>, else_body: Vec<Stmt>, span: Span) {
        self.hoisted.push(Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: Vec::new(),
                else_body: (!else_body.is_empty()).then_some(else_body),
            },
            span,
        ));
    }
}

/// Whether `expr` is a non-local `place ??= value` (the parser's `Assignment` with a lazy value
/// temporary over `place ?? value`) whose include sits only in `value`: the place, its read and
/// the prelude that stabilizes it run no include, so they can run ahead of the hoisted `if`.
fn coalesce_assignment_include_parts(expr: &Expr) -> Option<()> {
    let ExprKind::Assignment { target, value, result_target, prelude, conditional_value_temp } =
        &expr.kind
    else {
        return None;
    };
    conditional_value_temp.as_ref()?;
    let ExprKind::NullCoalesce { value: read, default } = &value.kind else {
        return None;
    };
    let clean = !expr_has_includes(target)
        && !expr_has_includes(read)
        && result_target.as_deref().is_none_or(|target| !expr_has_includes(target))
        && !super::contains::has_includes(prelude)
        && expr_has_includes(default);
    clean.then_some(())
}

/// Moves an operand holding an include into `[$t = operand;]`, returning that body and the
/// `$t` read that replaces it; an operand without one stays as it is.
fn operand_into_temp(operand: Expr, span: Span) -> (Vec<Stmt>, Expr) {
    if !expr_has_includes(&operand) {
        return (Vec::new(), operand);
    }
    let temp = fresh_temp();
    (vec![assign(&temp, operand, span)], variable(&temp, span))
}

/// Whether evaluating `expr` can have no observable effect (a literal, variable or constant).
fn is_inert(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::Variable(_)
            | ExprKind::ConstRef(_)
            | ExprKind::MagicConstant(_)
            | ExprKind::ClassConstant { .. }
    )
}

/// Whether `expr` reads a storage place without computing anything: a variable, `$this`, a
/// static property, a named property of a place, or a dimension of a place with an inert index.
fn is_place_read(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => is_place_read(object),
        ExprKind::ArrayAccess { array, index } => is_place_read(array) && is_inert(index),
        _ => false,
    }
}

/// Whether `expr` is a value no include can change: a literal, a constant, `$this`, or a
/// closure (whose creation runs none of its body).
fn is_inert_leaf(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::ConstRef(_)
            | ExprKind::MagicConstant(_)
            | ExprKind::This
            | ExprKind::ClassConstant { .. }
            | ExprKind::ScopedConstantAccess { .. }
            | ExprKind::Closure { .. }
    )
}

/// `$name`
fn variable(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

/// A fresh compiler-generated local for one hoisted value.
fn fresh_temp() -> String {
    crate::names::generated_local_name(&format!(
        "__elephc_include_value_{}",
        HOIST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// `$temp = value;`
fn assign(temp: &str, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: temp.to_string(),
            value,
        },
        span,
    )
}

