//! Purpose:
//! Decides which string values can still be the missing typed-array sentinel.
//!
//! Called from:
//! - `lower_cast` when a `(string)` cast's operand is already a string.
//! - Loop lowering, which records stores a back-edge will run before the next cast.
//!
//! Key details:
//! - Element reads publish the sentinel. Literals, concatenations, and scalar coercions do not.
//! - A local carries it only when a store into that slot forwarded such a value. A `string`
//!   parameter, and a local copied from one, stay real strings so `(string)` over them remains
//!   an alias.
//! - Stores later in a loop body are not emitted yet when the cast is lowered. The loop scan
//!   records those names from the AST. Straight-line stores after the cast are not included.
//! - Only a concrete string array publishes the sentinel, including through nested
//!   indexing (`$grid[0][$key]`). An untyped `array` read does not. `??` never
//!   yields the missing element, so only its default can carry the sentinel.
//! - A call or ternary stored into a local carries the sentinel when one of its arguments or
//!   branches does. A call that has not been stored may also return a sentinel its callee built.

use std::collections::HashSet;

use crate::ir::{Immediate, LocalSlotId, Op, ValueId};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::types::PhpType;

/// Returns whether a string value can still be the missing typed-array sentinel.
pub(super) fn string_value_may_carry_missing_entry(
    ctx: &LoweringContext<'_, '_>,
    value: LoweredValue,
) -> bool {
    let mut values = HashSet::new();
    let mut slots = HashSet::new();
    value_may_carry_missing_string(ctx, value.value, &mut values, &mut slots, false)
}

/// Follows ownership forwards to an element read that publishes the missing-string sentinel.
///
/// `stored` is set while deciding whether a value written into a local can carry the
/// sentinel. A store of an unrelated producer, such as a mixed clone of another
/// parameter, must not make every later load of that slot look like a missing entry.
fn value_may_carry_missing_string(
    ctx: &LoweringContext<'_, '_>,
    value: ValueId,
    values: &mut HashSet<ValueId>,
    slots: &mut HashSet<LocalSlotId>,
    stored: bool,
) -> bool {
    if !values.insert(value) {
        return false;
    }
    let Some(inst) = ctx.builder.value_defining_instruction(value) else {
        return false;
    };
    match inst.op {
        Op::ArrayGet | Op::ArrayGetSilent | Op::HashGet | Op::HashGetSilent => true,
        Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned | Op::StrPersist => inst
            .operands
            .first()
            .is_some_and(|source| {
                value_may_carry_missing_string(ctx, *source, values, slots, stored)
            }),
        Op::LoadLocal | Op::LoadStaticLocal => {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                return false;
            };
            if !slots.insert(slot) {
                return false;
            }
            let store = match inst.op {
                Op::LoadStaticLocal => Op::StoreStaticLocal,
                _ => Op::StoreLocal,
            };
            let stored_values = ctx
                .builder
                .function()
                .instructions
                .iter()
                .filter_map(|candidate| {
                    (candidate.op == store
                        && candidate.immediate == Some(Immediate::LocalSlot(slot)))
                        .then_some(candidate.operands.first().copied())
                        .flatten()
                })
                .collect::<Vec<_>>();
            if stored_values.into_iter().any(|stored_value| {
                value_may_carry_missing_string(ctx, stored_value, values, slots, true)
            }) {
                return true;
            }
            // The store that publishes the sentinel may sit later in this loop and
            // therefore not be in the instruction list yet.
            slot_name(ctx, slot).is_some_and(|name| ctx.loop_assigns_missing_string(&name))
        }
        Op::ConstStr
        | Op::StrConcat
        | Op::StrAppend
        | Op::StrInterpolate
        | Op::IToStr
        | Op::FToStr
        | Op::BoolToStr
        | Op::ResourceToStr
        | Op::MixedCastString
        | Op::CoreBuiltin
        | Op::RuntimeCall
        | Op::SplRuntimeCall
        | Op::FiberRuntimeCall
        | Op::StrCharAt
        | Op::Cast => false,
        Op::Call
        | Op::FunctionVariantCall
        | Op::LanguageConstructCall
        | Op::EvalLiteralCall
        | Op::EvalFunctionCall
        | Op::ClosureCall
        | Op::ExprCall
        | Op::MethodCall
        | Op::NullsafeMethodCall
        | Op::StaticMethodCall
        | Op::EvalStaticMethodCall
        | Op::PipeCall
        | Op::IteratorMethodCall => {
            let operands = inst.operands.clone();
            call_may_carry_missing_string(ctx, &operands, values, slots, stored)
        }
        // A producer reached only through a local store is a real value written into
        // the slot. The same producer used directly as the cast operand may still be
        // a sentinel the callee or branch built.
        _ => !stored,
    }
}

/// Returns whether a call result can be the missing-string sentinel.
///
/// A call that is itself the cast operand can return a sentinel its body built, so it
/// carries even when no argument does. Once that result is stored, only an argument
/// that already carries the sentinel keeps it: `pass($stored)` does, `label()` does not.
fn call_may_carry_missing_string(
    ctx: &LoweringContext<'_, '_>,
    operands: &[ValueId],
    values: &mut HashSet<ValueId>,
    slots: &mut HashSet<LocalSlotId>,
    stored: bool,
) -> bool {
    if !stored {
        return true;
    }
    operands.iter().any(|operand| {
        value_may_carry_missing_string(ctx, *operand, values, slots, true)
    })
}

/// Returns the PHP name stored on a local slot, when it has one.
fn slot_name(ctx: &LoweringContext<'_, '_>, slot: LocalSlotId) -> Option<String> {
    ctx.builder
        .function()
        .locals
        .iter()
        .find(|local| local.id == slot)
        .and_then(|local| local.name.clone())
}

/// Names a loop assigns a value that may still be the missing-string sentinel.
///
/// `condition` and `update` run on the back-edge as well as the body. The scan reads
/// stores that have already been emitted, so a miss saved before the loop is visible,
/// and it fixpoints through copies inside the loop. It does not look at code after the loop.
pub(crate) fn loop_region_missing_string_names(
    ctx: &LoweringContext<'_, '_>,
    body: &[Stmt],
    condition: Option<&Expr>,
    update: Option<&Stmt>,
) -> HashSet<String> {
    let mut carried = HashSet::new();
    loop {
        let mut found = HashSet::new();
        collect_stmts(body, &carried, ctx, &mut found);
        if let Some(condition) = condition {
            collect_expr(condition, &carried, ctx, &mut found);
        }
        if let Some(update) = update {
            collect_stmt(update, &carried, ctx, &mut found);
        }
        let before = carried.len();
        carried.extend(found);
        if carried.len() == before {
            break;
        }
    }
    carried
}

/// Returns whether a value already stored into `name` can be the sentinel.
fn emitted_local_may_carry(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    let Some(slot) = ctx.local_slots.get(name).copied() else {
        return false;
    };
    let mut values = HashSet::new();
    let mut slots = HashSet::new();
    if !slots.insert(slot) {
        return false;
    }
    let stored_values = ctx
        .builder
        .function()
        .instructions
        .iter()
        .filter_map(|candidate| {
            let stores = matches!(candidate.op, Op::StoreLocal | Op::StoreStaticLocal)
                && candidate.immediate == Some(Immediate::LocalSlot(slot));
            stores
                .then_some(candidate.operands.first().copied())
                .flatten()
        })
        .collect::<Vec<_>>();
    stored_values.into_iter().any(|stored_value| {
        value_may_carry_missing_string(ctx, stored_value, &mut values, &mut slots, true)
    })
}

/// Records locals this statement assigns from a value that may be the sentinel.
fn collect_stmt(
    stmt: &Stmt,
    carried: &HashSet<String>,
    ctx: &LoweringContext<'_, '_>,
    found: &mut HashSet<String>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_expr(value, carried, ctx, found);
            if expr_may_carry_missing_string(value, carried, ctx) {
                found.insert(name.clone());
            }
        }
        StmtKind::RefAssign { target, source } => {
            collect_expr(source, carried, ctx, found);
            if expr_may_carry_missing_string(source, carried, ctx) {
                found.insert(target.clone());
            }
        }
        StmtKind::StaticVar { name, init } => {
            collect_expr(init, carried, ctx, found);
            if expr_may_carry_missing_string(init, carried, ctx) {
                found.insert(name.clone());
            }
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::ConstDecl { value: expr, .. } => collect_expr(expr, carried, ctx, found),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr(condition, carried, ctx, found);
            collect_stmts(then_body, carried, ctx, found);
            for (condition, body) in elseif_clauses {
                collect_expr(condition, carried, ctx, found);
                collect_stmts(body, carried, ctx, found);
            }
            if let Some(body) = else_body {
                collect_stmts(body, carried, ctx, found);
            }
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            collect_stmts(then_body, carried, ctx, found);
            if let Some(body) = else_body {
                collect_stmts(body, carried, ctx, found);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            collect_expr(condition, carried, ctx, found);
            collect_stmts(body, carried, ctx, found);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt(init, carried, ctx, found);
            }
            if let Some(condition) = condition {
                collect_expr(condition, carried, ctx, found);
            }
            if let Some(update) = update {
                collect_stmt(update, carried, ctx, found);
            }
            collect_stmts(body, carried, ctx, found);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_expr(array, carried, ctx, found);
            collect_stmts(body, carried, ctx, found);
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_expr(subject, carried, ctx, found);
            for (labels, body) in cases {
                for label in labels {
                    collect_expr(label, carried, ctx, found);
                }
                collect_stmts(body, carried, ctx, found);
            }
            if let Some(body) = default {
                collect_stmts(body, carried, ctx, found);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_stmts(try_body, carried, ctx, found);
            for catch in catches {
                collect_stmts(&catch.body, carried, ctx, found);
            }
            if let Some(body) = finally_body {
                collect_stmts(body, carried, ctx, found);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::NamespaceBlock { body, .. } => {
            collect_stmts(body, carried, ctx, found);
        }
        StmtKind::Synthetic(body) => collect_stmts(body, carried, ctx, found),
        StmtKind::ArrayAssign { index, value, .. } => {
            collect_expr(index, carried, ctx, found);
            collect_expr(value, carried, ctx, found);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_expr(target, carried, ctx, found);
            collect_expr(value, carried, ctx, found);
        }
        StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            collect_expr(value, carried, ctx, found);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_expr(object, carried, ctx, found);
            collect_expr(value, carried, ctx, found);
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            collect_expr(object, carried, ctx, found);
            collect_expr(index, carried, ctx, found);
            collect_expr(value, carried, ctx, found);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_expr(index, carried, ctx, found);
            collect_expr(value, carried, ctx, found);
        }
        StmtKind::Include { path, .. } => collect_expr(path, carried, ctx, found),
        StmtKind::ListUnpack { value, .. } => collect_expr(value, carried, ctx, found),
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

/// Records carrying assignments in a statement list.
fn collect_stmts(
    stmts: &[Stmt],
    carried: &HashSet<String>,
    ctx: &LoweringContext<'_, '_>,
    found: &mut HashSet<String>,
) {
    for stmt in stmts {
        collect_stmt(stmt, carried, ctx, found);
    }
}

/// Records carrying assignments nested in an expression.
fn collect_expr(
    expr: &Expr,
    carried: &HashSet<String>,
    ctx: &LoweringContext<'_, '_>,
    found: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::Reference(inner) => collect_expr(inner, carried, ctx, found),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            collect_stmts(prelude, carried, ctx, found);
            collect_expr(target, carried, ctx, found);
            collect_expr(value, carried, ctx, found);
            if let Some(result) = result_target {
                collect_expr(result, carried, ctx, found);
            }
            if let ExprKind::Variable(name) = &target.kind {
                if expr_may_carry_missing_string(value, carried, ctx) {
                    found.insert(name.clone());
                }
            }
        }
        ExprKind::Closure { .. } => {}
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr(left, carried, ctx, found);
            collect_expr(right, carried, ctx, found);
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Clone(value)
        | ExprKind::Spread(value)
        | ExprKind::YieldFrom(value)
        | ExprKind::PtrCast { expr: value, .. } => collect_expr(value, carried, ctx, found),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_expr(value, carried, ctx, found);
            collect_expr(default, carried, ctx, found);
        }
        ExprKind::Pipe { value, callable } => {
            collect_expr(value, carried, ctx, found);
            collect_expr(callable, carried, ctx, found);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr(condition, carried, ctx, found);
            collect_expr(then_expr, carried, ctx, found);
            collect_expr(else_expr, carried, ctx, found);
        }
        ExprKind::Cast { expr, .. } | ExprKind::NamedArg { value: expr, .. } => {
            collect_expr(expr, carried, ctx, found);
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::NewGeneric { args, .. } => {
            for arg in args {
                collect_expr(arg, carried, ctx, found);
            }
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_expr(object, carried, ctx, found);
            for arg in args {
                collect_expr(arg, carried, ctx, found);
            }
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            collect_expr(object, carried, ctx, found);
            collect_expr(method, carried, ctx, found);
            for arg in args {
                collect_expr(arg, carried, ctx, found);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr(item, carried, ctx, found);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr(key, carried, ctx, found);
                collect_expr(value, carried, ctx, found);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                for expr in entry.exprs() {
                    collect_expr(expr, carried, ctx, found);
                }
            }
        }
        ExprKind::Match { subject, arms, default } => {
            collect_expr(subject, carried, ctx, found);
            for (labels, arm) in arms {
                for label in labels {
                    collect_expr(label, carried, ctx, found);
                }
                collect_expr(arm, carried, ctx, found);
            }
            if let Some(default) = default {
                collect_expr(default, carried, ctx, found);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr(array, carried, ctx, found);
            collect_expr(index, carried, ctx, found);
        }
        ExprKind::ClosureCall { args, .. } => {
            for arg in args {
                collect_expr(arg, carried, ctx, found);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_expr(callee, carried, ctx, found);
            for arg in args {
                collect_expr(arg, carried, ctx, found);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr(object, carried, ctx, found);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr(object, carried, ctx, found);
            collect_expr(property, carried, ctx, found);
        }
        ExprKind::NewDynamic { name_expr, args }
        | ExprKind::NewDynamicObject { class_name: name_expr, args, .. } => {
            collect_expr(name_expr, carried, ctx, found);
            for arg in args {
                collect_expr(arg, carried, ctx, found);
            }
        }
        ExprKind::BufferNew { len, .. } => collect_expr(len, carried, ctx, found),
        ExprKind::ObjectClassName { object } => collect_expr(object, carried, ctx, found),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_expr(key, carried, ctx, found);
            }
            if let Some(value) = value {
                collect_expr(value, carried, ctx, found);
            }
        }
        ExprKind::IncludeValue { path, .. } => collect_expr(path, carried, ctx, found),
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
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_)
        | ExprKind::StaticPropertyAccess { .. } => {}
    }
}

/// Returns whether an array read can publish the missing-string sentinel.
///
/// The sentinel is the element representation of a concrete string array. Nested
/// indexing uses the element type of the inner read, so `$grid[0][$key]` carries
/// when `$grid` is `array<array<string>>`. An untyped `array` read is null or
/// mixed, and a string offset is a real empty view.
fn array_access_may_carry_missing_string(
    array: &Expr,
    ctx: &LoweringContext<'_, '_>,
) -> bool {
    concrete_string_array(&array_value_type(array, ctx))
}

/// Returns the type of an array expression, including a nested index.
fn array_value_type(expr: &Expr, ctx: &LoweringContext<'_, '_>) -> PhpType {
    match &expr.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::ErrorSuppress(inner) | ExprKind::Print(inner) => array_value_type(inner, ctx),
        ExprKind::ArrayAccess { array, .. } => array_element_type(&array_value_type(array, ctx)),
        _ => PhpType::Mixed,
    }
}

/// Returns the element type of an array, or mixed when `ty` is not one.
fn array_element_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(element) => element.as_ref().clone(),
        PhpType::AssocArray { value, .. } => value.as_ref().clone(),
        _ => PhpType::Mixed,
    }
}

/// Returns whether `ty` stores string elements that use the missing-entry sentinel.
fn concrete_string_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Str,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Str,
        _ => false,
    }
}

/// Returns whether an expression's value can be the missing-string sentinel.
fn expr_may_carry_missing_string(
    expr: &Expr,
    carried: &HashSet<String>,
    ctx: &LoweringContext<'_, '_>,
) -> bool {
    match &expr.kind {
        ExprKind::ArrayAccess { array, .. } => {
            array_access_may_carry_missing_string(array, ctx)
        }
        ExprKind::ErrorSuppress(inner) | ExprKind::Print(inner) | ExprKind::Spread(inner) => {
            expr_may_carry_missing_string(inner, carried, ctx)
        }
        ExprKind::Variable(name) => {
            carried.contains(name) || emitted_local_may_carry(ctx, name)
        }
        ExprKind::Assignment { value, prelude, .. } => {
            prelude.iter().any(|stmt| stmt_result_may_carry(stmt, carried, ctx))
                || expr_may_carry_missing_string(value, carried, ctx)
        }
        ExprKind::NamedArg { value, .. } => expr_may_carry_missing_string(value, carried, ctx),
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            expr_may_carry_missing_string(then_expr, carried, ctx)
                || expr_may_carry_missing_string(else_expr, carried, ctx)
        }
        ExprKind::Match { arms, default, .. } => {
            arms.iter().any(|(_, arm)| expr_may_carry_missing_string(arm, carried, ctx))
                || default
                    .as_deref()
                    .is_some_and(|arm| expr_may_carry_missing_string(arm, carried, ctx))
        }
        // `??` uses the left side only when it is not the missing element, so that
        // side cannot publish the sentinel. The default still can.
        ExprKind::NullCoalesce { default, .. } => {
            expr_may_carry_missing_string(default, carried, ctx)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_may_carry_missing_string(value, carried, ctx)
                || expr_may_carry_missing_string(default, carried, ctx)
        }
        ExprKind::Cast { target, .. } if matches!(target, CastType::String) => false,
        ExprKind::BinaryOp { op: BinOp::Concat, .. } => false,
        ExprKind::BinaryOp { op, right, .. } => {
            matches!(op, BinOp::NullCoalesce)
                && expr_may_carry_missing_string(right, carried, ctx)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::MethodCall { args, .. }
        | ExprKind::NullsafeMethodCall { args, .. }
        | ExprKind::NullsafeDynamicMethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } => args
            .iter()
            .any(|arg| expr_may_carry_missing_string(arg, carried, ctx)),
        ExprKind::Pipe { value, .. } => expr_may_carry_missing_string(value, carried, ctx),
        _ => false,
    }
}

/// Returns whether a nested statement's value, not merely some assignment inside it, carries.
fn stmt_result_may_carry(
    stmt: &Stmt,
    carried: &HashSet<String>,
    ctx: &LoweringContext<'_, '_>,
) -> bool {
    match &stmt.kind {
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => {
            expr_may_carry_missing_string(expr, carried, ctx)
        }
        _ => false,
    }
}
