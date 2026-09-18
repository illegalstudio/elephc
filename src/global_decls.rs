//! Purpose:
//! Collects the PHP variable names that any function-like body in a program declares with
//! `global`, so the checker and EIR lowering answer "does program storage back this name?" from
//! ONE walk instead of two that can drift.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl` (once per check, before the first walk)
//! - `crate::ir_lower::function::lower_main` and the per-body lowering entry points
//!
//! Key details:
//! - `global $x;` inside a function/method/closure binds `$x` to the program-global cell the TOP
//!   LEVEL also writes through its own local slot. The checker must therefore not end a top-level
//!   binding of such a name (`unset` would leave the name unbound while another body still reaches
//!   the storage by name), and lowering must not abandon its slot. Both sides read this set.
//! - ONE walk, SHARED, so the checker can never approve a decision lowering refuses (or vice
//!   versa). It descends into every statement body AND every expression, so a `global` written
//!   inside a closure literal, an assignment prelude or an enum method counts exactly like one
//!   written in a named function.
//! - That expression descent used to be a deliberate blind spot, and both directions of widening
//!   it were measured and rejected at the time:
//!   - Widening the set LOWERING reads changes STORAGE CLASS: a top-level name moves into the
//!     `_eir_global_*` symbol and is typed `Mixed` there, and the array builtins then had
//!     `Mixed`-array backend gaps, so `$d = function () { global $a; }; $a = [3, 1, 2];
//!     echo implode(",", $a);` crashed and `array_sum`/`sort`/`usort`/`in_array`/`array_map`/
//!     `array_keys`/`array_reverse` on such a name were a hard `unsupported EIR backend feature`.
//!     Those gaps are closed: every one of those builtins now runs on a global-backed local.
//!   - Widening the set the CHECKER's `unset`-kill veto reads withholds the kill from every name
//!     a nested body merely MENTIONS, so `$a = $argc; unset($a); $f = function () { global $a; };
//!     $a = "s";` is a permissive retype warning and a hard `cannot reassign` under
//!     `--strict-locals`. That is the price of correctness: with the blind spot, the closure's
//!     write went to storage main no longer read, and a `set_error_handler` or `ob_start`
//!     callback that rebinds a global callable was silently ignored after its `try`.
//! - The walk is EXHAUSTIVE on `StmtKind` AND `ExprKind` on purpose. Every arm that carries no
//!   body is listed as a deliberate no-op rather than swept into a catch-all, so a new variant
//!   that CAN hold a body has to be classified here instead of silently becoming an unnoticed
//!   blind spot — which is how the enum-method arm went unexamined for a whole campaign.
//! - `StmtKind::PackedClassDecl` is not one of those blind spots at all: a packed class declares
//!   typed FIELDS only, with no method bodies and no expressions, so there is nothing in it to walk.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

/// Collects the PHP variable names that any function-like body in `statements` declares `global`.
///
/// The one set both EIR lowering and the checker's `unset`-kill veto read. A closure literal, an
/// assignment prelude and an enum method count like any named function — see the module preamble
/// for why the expression descent is part of the contract.
pub(crate) fn collect_global_var_names(statements: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_in_body(statements, &mut names);
    names
}

/// Recursively scans statement bodies for `global` declarations.
fn collect_in_body(statements: &[Stmt], names: &mut HashSet<String>) {
    for stmt in statements {
        collect_in_stmt(stmt, names);
    }
}

/// Scans one statement, every statement body nested inside it, and every expression it holds.
///
/// Exhaustive on `StmtKind`: a new statement that can carry a body or an expression has to be
/// classified here rather than defaulting to "declares nothing".
fn collect_in_stmt(stmt: &Stmt, names: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Global { vars } => {
            names.extend(vars.iter().cloned());
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_in_expr(condition, names);
            collect_in_body(then_body, names);
            for (condition, body) in elseif_clauses {
                collect_in_expr(condition, names);
                collect_in_body(body, names);
            }
            if let Some(body) = else_body {
                collect_in_body(body, names);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_in_body(then_body, names);
            if let Some(body) = else_body {
                collect_in_body(body, names);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            collect_in_expr(condition, names);
            collect_in_body(body, names);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_in_expr(array, names);
            collect_in_body(body, names);
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_in_expr(default, names);
                }
            }
            collect_in_body(body, names);
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => {
            collect_in_body(body, names);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_in_stmt(init, names);
            }
            if let Some(condition) = condition {
                collect_in_expr(condition, names);
            }
            if let Some(update) = update {
                collect_in_stmt(update, names);
            }
            collect_in_body(body, names);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_in_expr(subject, names);
            for (labels, body) in cases {
                for label in labels {
                    collect_in_expr(label, names);
                }
                collect_in_body(body, names);
            }
            if let Some(body) = default {
                collect_in_body(body, names);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_in_body(try_body, names);
            for catch in catches {
                collect_in_body(&catch.body, names);
            }
            if let Some(body) = finally_body {
                collect_in_body(body, names);
            }
        }
        // Method bodies are function-like bodies. Enum methods included: they run like class
        // methods and reach program storage the same way.
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            for method in methods {
                collect_in_body(&method.body, names);
            }
        }
        // Statements whose children are EXPRESSIONS: a closure literal or an assignment prelude
        // inside any of them can declare a global.
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::Include { path: expr, .. } => collect_in_expr(expr, names),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => collect_in_expr(value, names),
        StmtKind::RefAssign { source, .. } => collect_in_expr(source, names),
        StmtKind::StaticVar { init, .. } => collect_in_expr(init, names),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_in_expr(index, names);
            collect_in_expr(value, names);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_in_expr(target, names);
            collect_in_expr(value, names);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_in_expr(object, names);
            collect_in_expr(value, names);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_in_expr(object, names);
            collect_in_expr(index, names);
            collect_in_expr(value, names);
        }
        // A packed class declares typed FIELDS only — no bodies, no expressions, nothing to walk.
        StmtKind::PackedClassDecl { .. }
        // Externs are C declarations: types and names, never PHP statements.
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        // Leaves: no sub-statements and no sub-expressions.
        | StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        // Variant groups/marks carry function NAMES; the bodies live in their own `FunctionDecl`s.
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
    }
}

/// Scans one expression for the function-like bodies it holds.
///
/// Only two expression forms carry STATEMENTS: a closure literal's body and an assignment's
/// synthesized prelude. Every other arm merely recurses into its operands so a closure nested
/// anywhere inside an expression is still found. Exhaustive on `ExprKind` for the same reason
/// `collect_in_stmt` is exhaustive on `StmtKind`.
fn collect_in_expr(expr: &Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Closure { params, body, .. } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_in_expr(default, names);
                }
            }
            collect_in_body(body, names);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            collect_in_body(prelude, names);
            collect_in_expr(target, names);
            collect_in_expr(value, names);
            if let Some(result_target) = result_target {
                collect_in_expr(result_target, names);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_in_expr(left, names);
            collect_in_expr(right, names);
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
        | ExprKind::YieldFrom(value) => collect_in_expr(value, names),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_in_expr(value, names);
            collect_in_expr(default, names);
        }
        ExprKind::Pipe { value, callable } => {
            collect_in_expr(value, names);
            collect_in_expr(callable, names);
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_in_expr(array, names);
            collect_in_expr(index, names);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_in_expr(object, names);
            collect_in_expr(property, names);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_in_expr(condition, names);
            collect_in_expr(then_expr, names);
            collect_in_expr(else_expr, names);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_in_expr(subject, names);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_in_expr(condition, names);
                }
                collect_in_expr(result, names);
            }
            if let Some(default) = default {
                collect_in_expr(default, names);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::ArrayLiteral(args) => {
            for arg in args {
                collect_in_expr(arg, names);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                collect_in_expr(key, names);
                collect_in_expr(value, names);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_in_expr(callee, names);
            for arg in args {
                collect_in_expr(arg, names);
            }
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
            collect_in_expr(receiver, names);
            for arg in args {
                collect_in_expr(arg, names);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_in_expr(object, names);
            collect_in_expr(method, names);
            for arg in args {
                collect_in_expr(arg, names);
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_in_expr(key, names);
            }
            if let Some(value) = value {
                collect_in_expr(value, names);
            }
        }
        // Leaves: literals, names, receivers and constants hold no expression a closure could
        // hide in. A static receiver names a class; the call's arguments are walked above.
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
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
        | ExprKind::MagicConstant(_) => {}
    }
}
