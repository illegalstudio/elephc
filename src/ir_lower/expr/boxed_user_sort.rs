//! Purpose:
//! Sorts boxed PHP arrays through a private dense working array and a rooted comparator.
//!
//! Called from:
//! - Direct and statically resolved callable builtin lowering before property write-back rewriting.
//!
//! Key details:
//! - Argument expressions run in source order before the referenced array is copied.
//! - Comparators see the original receiver, with the working array published on return or throw.
//! - Reference anchors and object roots preserve the destination across callback side effects.

use super::*;
use crate::ir::{RuntimeCallTarget, RuntimeFnId};
use crate::types::call_args::{plan_call_args, PlannedRegularArg};

/// A stable receiver binding and the optional object owner keeping its property cell alive.
struct SortPlace {
    target: Expr,
    alias: Option<String>,
    owner: Option<String>,
}

/// Tracks whether argument validation reached the point where sorting owns a publishable copy.
struct SortGuard {
    handler: BlockId,
    started: String,
}

/// Normalizes boxed receivers without changing raw-array specializations or spread diagnostics.
pub(super) fn lower_boxed_usort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "usort"
        || ctx.functions.contains_key(name)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let sig = call_signature(ctx, name, false)?;
    let plan = plan_call_args(&sig, args, expr.span, false, false).ok()?;
    let [PlannedRegularArg::Source { source_index: array_index, expr: array },
        PlannedRegularArg::Source { source_index: callback_index, expr: callback }] =
        plan.regular_args.as_slice() else { return None; };
    if ref_place_args::static_place_type(ctx, array)?.codegen_repr() != PhpType::Mixed
        || !supported_place(ctx, array)
    {
        return None;
    }

    let handler = ctx.builder.create_named_block("usort.catch", Vec::new());
    let started = ctx.declare_hidden_temp(PhpType::Bool);
    let no = lower_bool_literal(ctx, false, expr);
    ctx.store_local(&started, no, PhpType::Bool, Some(expr.span));
    let guard = SortGuard { handler, started };
    handler_op(ctx, Op::TryPushHandler, guard.handler, expr.span);
    let (place, callback_source) = if array_index < callback_index {
        let place = capture_place(ctx, array);
        (place, root_expression(ctx, callback))
    } else {
        let callback_source = root_expression(ctx, callback);
        (capture_place(ctx, array), callback_source)
    };
    // Capturing a reference must not take a snapshot before a later callback
    // factory runs: that factory can replace the referenced PHP array.
    let source = lower_expr(ctx, &place.target);
    require_array(ctx, source, expr);
    let work_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let work = ctx.emit_owned_value(
        Op::RuntimeCall, vec![source.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayValues))),
        work_ty.clone(), effects_lookup::runtime_effects(), Some(expr.span),
    );
    let work_slot = ctx.declare_hidden_temp(work_ty.clone());
    ctx.store_local(&work_slot, work, work_ty, Some(expr.span));

    let callback_value = ctx.load_local(&callback_source, Some(expr.span));
    let descriptor = ctx.emit_owned_value(
        Op::NormalizeCallable, vec![callback_value.value], None, PhpType::Callable,
        Op::NormalizeCallable.default_effects(), Some(expr.span),
    );
    let descriptor_slot = ctx.declare_hidden_temp(PhpType::Callable);
    ctx.store_local(&descriptor_slot, descriptor, PhpType::Callable, Some(expr.span));
    sort_and_publish(ctx, &place, &work_slot, &descriptor_slot, &callback_source, &guard, expr);
    Some(lower_null(ctx, expr))
}

/// Selects places whose stable reference identity can be represented by existing EIR primitives.
fn supported_place(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, property } => {
            let Some(ty) = ref_place_args::static_place_type(ctx, object) else { return false; };
            let PhpType::Object(class) = ty.codegen_repr() else { return false; };
            ctx.classes.get(class.as_str()).is_some_and(|info| {
                info.reference_properties.contains(property)
                    && !info.readonly_properties.contains(property)
                    && !info.methods.contains_key(&php_symbol_key(&property_hook_get_method(property)))
                    && !info.methods.contains_key(&php_symbol_key(&crate::names::property_hook_set_method(property)))
            })
        }
        _ => false,
    }
}

/// Captures a variable or promoted property reference without reading its current array value.
fn capture_place(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> SortPlace {
    match &expr.kind {
        ExprKind::Variable(source) => {
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            ctx.alias_local_ref_cell(&alias, source, Some(expr.span));
            SortPlace {
                target: Expr::new(ExprKind::Variable(alias.clone()), expr.span),
                alias: Some(alias), owner: None,
            }
        }
        ExprKind::PropertyAccess { object, property } => {
            let owner = root_expression(ctx, object);
            let target = Expr::new(ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::Variable(owner.clone()), object.span)),
                property: property.clone(),
            }, expr.span);
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            property_access::lower_ref_assign_property(ctx, &alias, &target, expr.span);
            SortPlace {
                target: Expr::new(ExprKind::Variable(alias.clone()), expr.span),
                alias: Some(alias), owner: Some(owner),
            }
        }
        _ => SortPlace { target: expr.clone(), alias: None, owner: None },
    }
}

/// Roots an argument expression before later arguments, callbacks or exceptions can release it.
fn root_expression(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> String {
    let value = lower_expr(ctx, expr);
    let ty = ctx.builder.value_php_type(value.value);
    let slot = ctx.declare_hidden_temp(ty.clone());
    store_value_into_temp(ctx, &slot, ty, value, expr.span);
    slot
}

/// Rejects non-arrays before callback validation and before any receiver mutation.
fn require_array(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) {
    let valid = emit_builtin_call_value(ctx, "is_array", vec![value.value], PhpType::Bool, expr.span, None);
    let ready = ctx.builder.create_named_block("usort.array.valid", Vec::new());
    let invalid = ctx.builder.create_named_block("usort.array.invalid", Vec::new());
    conditional(ctx, valid.value, ready, invalid);
    ctx.builder.position_at_end(invalid);
    let exception = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified("TypeError"),
        args: vec![Expr::new(ExprKind::StringLiteral(
            "usort(): Argument #1 ($array) must be of type array".to_string(),
        ), expr.span)],
    }, expr.span));
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(ready);
}

/// Sorts a private array, then publishes it through one shared normal/exception finalizer.
fn sort_and_publish(
    ctx: &mut LoweringContext<'_, '_>,
    place: &SortPlace,
    work: &str,
    descriptor: &str,
    callback_source: &str,
    guard: &SortGuard,
    expr: &Expr,
) {
    let finalize = ctx.builder.create_named_block("usort.publish", Vec::new());
    let commit = ctx.builder.create_named_block("usort.commit", Vec::new());
    let cleanup = ctx.builder.create_named_block("usort.cleanup", Vec::new());
    let rethrow = ctx.builder.create_named_block("usort.rethrow", Vec::new());
    let done = ctx.builder.create_named_block("usort.done", Vec::new());
    let exception_ty = PhpType::Object("Throwable".to_string());
    let exception_slot = ctx.declare_owned_hidden_temp(exception_ty.clone());
    let failed_slot = ctx.declare_hidden_temp(PhpType::Bool);
    let no = lower_bool_literal(ctx, false, expr);
    ctx.store_local(&failed_slot, no, PhpType::Bool, Some(expr.span));
    let yes = lower_bool_literal(ctx, true, expr);
    ctx.store_local(&guard.started, yes, PhpType::Bool, Some(expr.span));
    let array = ctx.load_local(work, Some(expr.span));
    let callback = ctx.load_local(descriptor, Some(expr.span));
    // The existing raw sorter COW-separates this rooted working slot. No
    // callback can observe its payload through the original receiver.
    emit_builtin_call_value(ctx, "usort", vec![array.value, callback.value], PhpType::Void, expr.span, None);
    handler_op(ctx, Op::TryPopHandler, guard.handler, expr.span);
    branch(ctx, finalize);

    ctx.builder.position_at_end(guard.handler);
    ctx.clear_static_callable_locals();
    handler_op(ctx, Op::TryPopHandler, guard.handler, expr.span);
    let exception = ctx.emit_owned_value(
        Op::CatchBind, Vec::new(), None, exception_ty.clone(),
        Op::CatchBind.default_effects(), Some(expr.span),
    );
    ctx.store_local(&exception_slot, exception, exception_ty, Some(expr.span));
    let yes = lower_bool_literal(ctx, true, expr);
    ctx.store_local(&failed_slot, yes, PhpType::Bool, Some(expr.span));
    branch(ctx, finalize);

    ctx.builder.position_at_end(finalize);
    let started = ctx.load_local(&guard.started, Some(expr.span));
    conditional(ctx, started.value, commit, cleanup);
    ctx.builder.position_at_end(commit);
    publish(ctx, place, work, expr);
    branch(ctx, cleanup);
    ctx.builder.position_at_end(cleanup);
    release_root(ctx, work, expr.span);
    release_root(ctx, descriptor, expr.span);
    release_root(ctx, callback_source, expr.span);
    if let Some(alias) = &place.alias {
        let null = lower_null(ctx, expr);
        ctx.unset_local(alias, null, Some(expr.span));
    }
    if let Some(owner) = &place.owner {
        release_root(ctx, owner, expr.span);
    }
    let failed = ctx.load_local(&failed_slot, Some(expr.span));
    conditional(ctx, failed.value, rethrow, done);
    ctx.builder.position_at_end(rethrow);
    let exception = take_owned_temp(ctx, &exception_slot, expr.span);
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(done);
    ctx.clear_static_callable_locals();
}

/// Boxes the sorted dense payload and replaces the captured receiver without changing its layout.
fn publish(ctx: &mut LoweringContext<'_, '_>, place: &SortPlace, work: &str, expr: &Expr) {
    let array = ctx.load_local(work, Some(expr.span));
    let boxed = ctx.emit_owned_value(
        Op::MixedBox, vec![array.value], None, PhpType::Mixed,
        Op::MixedBox.default_effects(), Some(expr.span),
    );
    if let Some(alias) = &place.alias {
        ctx.store_local(alias, boxed, PhpType::Mixed, Some(expr.span));
    } else {
        let slot = ctx.declare_hidden_temp(PhpType::Mixed);
        ctx.store_local(&slot, boxed, PhpType::Mixed, Some(expr.span));
        lower_non_local_assignment_write(ctx, &place.target,
            &Expr::new(ExprKind::Variable(slot.clone()), expr.span), expr.span);
        release_root(ctx, &slot, expr.span);
    }
}

/// Clears a rooted slot before releasing its occupant so a throwing destructor cannot revisit it.
fn release_root(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    let slot = ctx.declare_local(name, ctx.local_type(name));
    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
}

/// Pushes or pops the matching runtime handler around comparator execution.
fn handler_op(ctx: &mut LoweringContext<'_, '_>, op: Op, handler: BlockId, span: Span) {
    ctx.emit_void(op, Vec::new(), Some(Immediate::I64(handler.as_raw() as i64)), op.default_effects(), Some(span));
}

/// Joins normal and exceptional control flow without duplicating ownership retirement.
fn branch(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
}

/// Selects the validation or exception continuation from a boolean EIR value.
fn conditional(ctx: &mut LoweringContext<'_, '_>, cond: ValueId, then_target: BlockId, else_target: BlockId) {
    ctx.builder.terminate(Terminator::CondBr {
        cond, then_target, then_args: Vec::new(), else_target, else_args: Vec::new(),
    });
}
