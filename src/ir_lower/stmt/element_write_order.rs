//! Purpose:
//! Decides when an element write must fetch its receiver AFTER lowering the key and the value.
//!
//! Called from:
//! - `crate::ir_lower::stmt::array_write_core::lower_array_assign_with_diagnosed_key()`.
//! - `crate::ir_lower::stmt::array_write_storage::lower_array_push()`.
//! - `crate::ir_lower::stmt::property_array_writes` (`$o->p[k] = v`, `$o->p[] = v`).
//! - `crate::ir_lower::stmt::static_property_writes` (`C::$p[k] = v`, `C::$p[] = v`).
//!
//! Key details:
//! - PHP evaluates `$a[k] = v` as: the key expression, the value, then a fetch of `$a` for
//!   writing. elephc fetches the receiver first, which is only observable when the key or the
//!   value writes the receiver variable: `$m["k"] = ($m = [...]) ? ... : ...`. The early fetch
//!   then holds the array that the reassignment released, and the write lands in freed memory
//!   (garbage keys, or a hang).
//! - The early fetch is kept everywhere else, deliberately. A `Mixed` receiver is detached
//!   (`MixedClone`) and stored back when it is fetched for writing. A value that borrows from
//!   the receiver (`$m["c"] = $m["a"]`) is safe only if it is read from the detached copy, so
//!   moving every fetch later would trade this bug for a use-after-free in the common case.
//! - The rule: the receiver is fetched late when a key or value expression may write the
//!   receiver local. That covers the cases below, and is conservative everywhere else:
//!   - it names the local as a write target (assignment, list, compound, increment), passes it
//!     where a callee can take it by reference, or captures it by reference in a closure
//!     (`expr_writes_local`);
//!   - it calls `eval()`, `extract()` or `parse_str()`, which write locals by name;
//!   - the receiver is reachable other than by its name (a reference-bound, global or static
//!     local) and the expression contains an explicit call, `new`, `clone`, `include` or
//!     `yield`, any of which can then write it;
//!   - the receiver is reference-bound and the expression writes any variable, which may be
//!     one of its aliases (`$r = &$m; $m["k"] = ($r = [...]) ? ...`).
//! - A property receiver (`$o->p[k] = v`, `C::$p[k] = v`) is fetched late when an operand
//!   assigns a property of that name (on any object or class, since aliasing cannot be ruled
//!   out), makes an explicit call (a method such as `rebuild()` may reset `$this->cache`), or
//!   calls `eval()`. A property fetch has no detach step that a borrowed value depends on, so
//!   the broader trigger is safe there.
//! - A magic method reached implicitly (`__get` from a property read, `offsetGet` from an
//!   offset read, `__toString` from a cast or a concatenation), or a destructor run by an
//!   overwrite inside the operand, does not count for an aliased receiver. Counting them would
//!   move the fetch for ordinary top-level code such as `$m["c"] = $m["a"]` (top-level locals
//!   are global storage) onto the late path. Such a hook that writes the receiver through
//!   `global`, `$GLOBALS` or a reference still sees the early fetch; `docs/php/types.md` lists it.
//! - Every heap operand on the late path is pinned in an unwind-visible slot as soon as it is
//!   lowered (`lower_pinned_write_key_and_value`, `root_call_operand`): a later operand may free
//!   the array an earlier one borrows from, or throw while an earlier one holds an owned
//!   temporary.
//! - A nested write (`$m["a"]["b"] = v`) uses the same gates for the chain's root when every
//!   index is free of side effects, and runs the value first
//!   (`nested_array_writes::lower_nested_array_assign`).

use crate::ir::LocalKind;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use super::*;

/// Returns true when any of `operands` (the key and value of an element write) may write the
/// receiver local `receiver`, so the receiver must be fetched after they are lowered.
pub(super) fn element_write_operands_may_write_receiver(
    ctx: &LoweringContext<'_, '_>,
    receiver: &str,
    operands: &[&Expr],
) -> bool {
    let aliased = receiver_is_aliased(ctx, receiver);
    operands.iter().any(|operand| {
        crate::ir_lower::expr::expr_writes_local(operand, receiver)
            || expr_assigns_through_local(operand, receiver)
            || crate::ir_lower::expr::expr_contains_eval_call(operand)
            || calls_a_local_scope_writer(operand)
            || (aliased && expr_contains_call(operand))
            || (ctx.is_ref_bound_local(receiver) && expr_writes_any_variable(operand))
    })
}

/// Returns true when a key or value of a write into `$object->property[...]` may reassign that
/// property before the write, so the property must be fetched after them. It may when it
/// assigns a property of that name (on any object: aliasing cannot be ruled out here), makes an
/// explicit call (a method such as `rebuild()` can reset `$this->cache`), writes any variable
/// (it may be a reference alias of the property: `$r = &$o->arr; ... ($r = [...])`), or calls
/// `eval()`.
pub(super) fn property_element_operands_may_write_property(property: &str, operands: &[&Expr]) -> bool {
    operands.iter().any(|operand| {
        expr_assigns_instance_property(operand, property)
            || expr_writes_any_variable(operand)
            || expr_contains_call(operand)
            || crate::ir_lower::expr::expr_contains_eval_call(operand)
    })
}

/// Returns true when a key or value of a write into `Class::$property[...]` may reassign that
/// static property first: it assigns a static property of that name, writes any variable (a
/// possible reference alias), makes an explicit call, or calls `eval()`.
pub(super) fn static_property_element_operands_may_write_property(
    property: &str,
    operands: &[&Expr],
) -> bool {
    operands.iter().any(|operand| {
        expr_assigns_static_property(operand, property)
            || expr_writes_any_variable(operand)
            || expr_contains_call(operand)
            || crate::ir_lower::expr::expr_contains_eval_call(operand)
    })
}

/// Returns true when the local can be written other than through its own name: a reference
/// binding, program-global storage (reached by `global` and `$GLOBALS` from any function), or a
/// function-static slot (reached by a recursive call).
fn receiver_is_aliased(ctx: &LoweringContext<'_, '_>, receiver: &str) -> bool {
    ctx.is_ref_bound_local(receiver)
        || ctx.local_uses_global_storage(receiver)
        || ctx.local_kinds.get(receiver) == Some(&LocalKind::StaticLocal)
}

/// Returns true when `expr` calls `extract()` or `parse_str()`, which write caller locals by
/// name, directly or through a callable string (`call_user_func("extract", ...)`), which counts
/// as a match wherever the name appears as a string literal. `eval()` is checked separately.
fn calls_a_local_scope_writer(expr: &Expr) -> bool {
    let is_writer = |name: &str| {
        let name = name.trim_start_matches('\\');
        name.eq_ignore_ascii_case("extract") || name.eq_ignore_ascii_case("parse_str")
    };
    expr_any(expr, &|node| match &node.kind {
        ExprKind::FunctionCall { name, .. } => name.last_segment().is_some_and(is_writer),
        ExprKind::StringLiteral(value) => is_writer(value),
        _ => false,
    })
}

/// Returns true when `expr` assigns a place rooted at the local `receiver` other than the local
/// itself: an element, a nested element or a destructured element of it (`$m["b"] = 5`,
/// `$m[1] += 5`, `[$m[1]] = [9]`). `expr_writes_local` covers writes to the local by name.
///
/// An assignment may hide its store in its `prelude`: a nested `++`/`--` desugars to an
/// assignment whose element store (`NestedArrayAssign`) lives there, so the walk has to reach it
/// or `$m[0] = ++$m[1][0]` fetches the receiver before the value rewrites it.
fn expr_assigns_through_local(expr: &Expr, receiver: &str) -> bool {
    expr_any(expr, &|node| match &node.kind {
        ExprKind::Assignment { target, prelude, .. } => {
            place_root_is(target, &|root| is_local_named(root, receiver))
                || prelude_assigns_through_local(prelude, receiver)
        }
        _ => false,
    })
}

/// Returns true when `expr` is exactly the bare local `receiver`.
fn is_local_named(expr: &Expr, receiver: &str) -> bool {
    matches!(&expr.kind, ExprKind::Variable(name) if name == receiver)
}

/// Returns true when any statement of an assignment prelude stores into a place rooted at the
/// local `receiver`.
fn prelude_assigns_through_local(prelude: &[Stmt], receiver: &str) -> bool {
    prelude
        .iter()
        .any(|stmt| stmt_assigns_through_local(stmt, receiver))
}

/// Returns true when `stmt` stores into a place rooted at the local `receiver`.
///
/// Only stores that can reach the local are matched; a nested declaration body is a separate
/// scope and is skipped, and every other shape answers false. The match is exhaustive so a new
/// `StmtKind` has to decide here rather than silently staying "not a write".
fn stmt_assigns_through_local(stmt: &Stmt, receiver: &str) -> bool {
    let root_is_receiver = |root: &Expr| is_local_named(root, receiver);
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            name == receiver || expr_assigns_through_local(value, receiver)
        }
        StmtKind::ArrayAssign { array, index, value } => {
            array == receiver
                || expr_assigns_through_local(index, receiver)
                || expr_assigns_through_local(value, receiver)
        }
        StmtKind::ArrayPush { array, value } => {
            array == receiver || expr_assigns_through_local(value, receiver)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            place_root_is(target, &root_is_receiver)
                || expr_assigns_through_local(value, receiver)
        }
        StmtKind::ListUnpack { vars, value } => {
            vars.iter().any(|var| var == receiver)
                || expr_assigns_through_local(value, receiver)
        }
        StmtKind::RefAssign { target, source } => {
            target == receiver || expr_assigns_through_local(source, receiver)
        }
        StmtKind::StaticVar { name, init } => {
            name == receiver || expr_assigns_through_local(init, receiver)
        }
        StmtKind::Global { vars } => vars.iter().any(|var| var == receiver),
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            expr_assigns_through_local(expr, receiver)
        }
        StmtKind::Return(expr) => expr
            .as_ref()
            .is_some_and(|expr| expr_assigns_through_local(expr, receiver)),
        StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            expr_assigns_through_local(index, receiver)
                || expr_assigns_through_local(value, receiver)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_assigns_through_local(object, receiver)
                || expr_assigns_through_local(value, receiver)
        }
        StmtKind::Include { path, .. } => expr_assigns_through_local(path, receiver),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_assigns_through_local(condition, receiver)
                || prelude_assigns_through_local(then_body, receiver)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_assigns_through_local(condition, receiver)
                        || prelude_assigns_through_local(body, receiver)
                })
                || else_body
                    .as_ref()
                    .is_some_and(|body| prelude_assigns_through_local(body, receiver))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            prelude_assigns_through_local(then_body, receiver)
                || else_body
                    .as_ref()
                    .is_some_and(|body| prelude_assigns_through_local(body, receiver))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_assigns_through_local(condition, receiver)
                || prelude_assigns_through_local(body, receiver)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_assigns_through_local(stmt, receiver))
                || condition
                    .as_ref()
                    .is_some_and(|expr| expr_assigns_through_local(expr, receiver))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_assigns_through_local(stmt, receiver))
                || prelude_assigns_through_local(body, receiver)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_assigns_through_local(array, receiver)
                || prelude_assigns_through_local(body, receiver)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_assigns_through_local(subject, receiver)
                || cases.iter().any(|(patterns, body)| {
                    patterns
                        .iter()
                        .any(|pattern| expr_assigns_through_local(pattern, receiver))
                        || prelude_assigns_through_local(body, receiver)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| prelude_assigns_through_local(body, receiver))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            prelude_assigns_through_local(try_body, receiver)
                || catches
                    .iter()
                    .any(|clause| prelude_assigns_through_local(&clause.body, receiver))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| prelude_assigns_through_local(body, receiver))
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => prelude_assigns_through_local(body, receiver),
        // A nested function or class body has its own scope: the enclosing local is not visible
        // there unless it is captured, which `ExprKind::Closure` handles on the expression side.
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

/// Returns true when evaluating `expr` makes an explicit call into code that can reach an
/// aliased local or an object property: a function, method, static or closure call, `new` (a
/// constructor), `clone` (`__clone`), a pipe, `include`, or a `yield` (the generator's consumer
/// runs while it is suspended). A closure DEFINITION calls nothing.
pub(super) fn expr_contains_call(expr: &Expr) -> bool {
    expr_any(expr, &|node| {
        matches!(
            node.kind,
            ExprKind::FunctionCall { .. }
                | ExprKind::MethodCall { .. }
                | ExprKind::NullsafeMethodCall { .. }
                | ExprKind::NullsafeDynamicMethodCall { .. }
                | ExprKind::StaticMethodCall { .. }
                | ExprKind::ClosureCall { .. }
                | ExprKind::ExprCall { .. }
                | ExprKind::NewObject { .. }
                | ExprKind::NewScopedObject { .. }
                | ExprKind::NewDynamic { .. }
                | ExprKind::NewDynamicObject { .. }
                | ExprKind::Clone(_)
                | ExprKind::Pipe { .. }
                | ExprKind::IncludeValue { .. }
                | ExprKind::Yield { .. }
                | ExprKind::YieldFrom(_)
        ) || assignment_prelude_is_opaque(node)
    })
}

/// Returns true when `expr` writes any variable: an assignment (list and compound forms
/// included, since the parser lowers them to assignments) or an increment or decrement.
fn expr_writes_any_variable(expr: &Expr) -> bool {
    expr_any(expr, &|node| {
        matches!(
            node.kind,
            ExprKind::Assignment { .. }
                | ExprKind::PreIncrement(_)
                | ExprKind::PostIncrement(_)
                | ExprKind::PreDecrement(_)
                | ExprKind::PostDecrement(_)
        )
    })
}

/// Returns true when `expr` assigns an instance property named `property` on any object, or a
/// place under one (`$o->arr = ...`, `$o->arr["k"] = ...`).
pub(super) fn expr_assigns_instance_property(expr: &Expr, property: &str) -> bool {
    expr_any(expr, &|node| {
        assignment_prelude_is_opaque(node)
            || matches!(&node.kind, ExprKind::Assignment { target, .. }
                if place_root_is(target, &|root| matches!(&root.kind,
                    ExprKind::PropertyAccess { property: name, .. }
                    | ExprKind::NullsafePropertyAccess { property: name, .. } if name == property)
                    || matches!(root.kind, ExprKind::DynamicPropertyAccess { .. })))
    })
}

/// Returns true when `expr` assigns a static property named `property` on any class, or a place
/// under one (`S::$arr = ...`).
pub(super) fn expr_assigns_static_property(expr: &Expr, property: &str) -> bool {
    expr_any(expr, &|node| {
        assignment_prelude_is_opaque(node)
            || matches!(&node.kind, ExprKind::Assignment { target, .. }
                if place_root_is(target, &|root| matches!(&root.kind,
                    ExprKind::StaticPropertyAccess { property: name, .. } if name == property)))
    })
}

/// Returns true when the place `target` (an assignment target), or a container it indexes into,
/// satisfies `is_root`.
fn place_root_is(target: &Expr, is_root: &dyn Fn(&Expr) -> bool) -> bool {
    if is_root(target) {
        return true;
    }
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => place_root_is(array, is_root),
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| place_root_is(item, is_root)),
        ExprKind::ArrayLiteralAssoc(entries) => {
            entries.iter().any(|(_, item)| place_root_is(item, is_root))
        }
        _ => false,
    }
}

/// Returns true for an assignment whose statement prelude this walk cannot see into; it is
/// counted as a match so the caller stays conservative.
fn assignment_prelude_is_opaque(node: &Expr) -> bool {
    matches!(&node.kind, ExprKind::Assignment { prelude, .. } if !prelude.is_empty())
}

/// Returns true when `pred` holds for any of `exprs` or anything nested in them.
fn any_of<'a>(exprs: impl IntoIterator<Item = &'a Expr>, pred: &dyn Fn(&Expr) -> bool) -> bool {
    exprs.into_iter().any(|expr| expr_any(expr, pred))
}

/// Returns true when `pred` holds for `expr` or any expression nested in it. A closure body is
/// not walked: defining a closure evaluates nothing inside it. The match is exhaustive so a new
/// expression kind has to decide here.
fn expr_any(expr: &Expr, pred: &dyn Fn(&Expr) -> bool) -> bool {
    if pred(expr) {
        return true;
    }
    match &expr.kind {
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::NewGeneric { args, .. } => any_of(args.iter(), pred),
        ExprKind::MethodCall { object, args, .. } | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_any(object, pred) || any_of(args.iter(), pred)
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            expr_any(object, pred) || expr_any(method, pred) || any_of(args.iter(), pred)
        }
        ExprKind::ExprCall { callee, args } => expr_any(callee, pred) || any_of(args.iter(), pred),
        ExprKind::NewDynamic { name_expr, args } => expr_any(name_expr, pred) || any_of(args.iter(), pred),
        ExprKind::NewDynamicObject { class_name, args, .. } => {
            expr_any(class_name, pred) || any_of(args.iter(), pred)
        }
        ExprKind::Assignment { target, value, result_target, .. } => {
            expr_any(target, pred)
                || expr_any(value, pred)
                || result_target.as_ref().is_some_and(|target| expr_any(target, pred))
        }
        ExprKind::BinaryOp { left, right, .. } => expr_any(left, pred) || expr_any(right, pred),
        ExprKind::InstanceOf { value, .. } => expr_any(value, pred),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. } => expr_any(inner, pred),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe { value, callable: default }
        | ExprKind::ArrayAccess { array: value, index: default } => {
            expr_any(value, pred) || expr_any(default, pred)
        }
        ExprKind::ArrayLiteral(items) => any_of(items.iter(), pred),
        ExprKind::ArrayLiteralAssoc(entries) => {
            entries.iter().any(|(key, value)| expr_any(key, pred) || expr_any(value, pred))
        }
        ExprKind::ArrayLiteralMixed(entries) => any_of(entries.iter().flat_map(|entry| entry.exprs()), pred),
        ExprKind::Match { subject, arms, default } => {
            expr_any(subject, pred)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(|pattern| expr_any(pattern, pred)) || expr_any(value, pred)
                })
                || default.as_ref().is_some_and(|default| expr_any(default, pred))
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr_any(condition, pred) || expr_any(then_expr, pred) || expr_any(else_expr, pred)
        }
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_any(object, pred)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_any(object, pred) || expr_any(property, pred)
        }
        ExprKind::Yield { key, value } => {
            key.as_ref().is_some_and(|key| expr_any(key, pred))
                || value.as_ref().is_some_and(|value| expr_any(value, pred))
        }
        ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::StringLiteral(_)
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
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}
