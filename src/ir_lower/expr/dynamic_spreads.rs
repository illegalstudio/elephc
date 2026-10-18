//! Purpose:
//! Materializes runtime unpack streams for fixed signatures and hidden argument collectors.
//!
//! Called from:
//! - Shared signature-based argument lowering and statically selected builtin callable targets.
//!
//! Key details:
//! - Consumes the shared source plan, then binds runtime keys without discarding named arguments.
//! - Preserve-values builtins retain optional null slots and defer coercion to their coordinator.
//! - Iterator sources and argument values stay in rooted slots across later evaluation and throws.

use super::*;

/// Tracks runtime parameter occupancy separately from the next positional argument index.
struct SpreadBindings {
    slots: Vec<String>,
    filled: Vec<String>,
    next: String,
    named: String,
    actual: String,
    collector: Option<String>,
    span: Span,
}

/// An evaluated argument stays rooted until every source expression has completed.
enum StagedSpreadArgument {
    Spread(String),
    Named(usize, String),
    Positional(String),
}

/// Lowers unpack sources for by-value fixed signatures and compiler-owned surplus collectors.
pub(super) fn lower_boxed_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    builtin: &str,
    preserve_values: bool,
) -> Option<Vec<ValueId>> {
    let hidden = crate::func_args::sig_collects_surplus_args(sig);
    if sig.variadic.is_some() && !hidden || sig.ref_params.iter().any(|by_ref| *by_ref)
        || !hidden && !preserve_values && sig.defaults.iter().any(Option::is_some)
        || crate::func_args::sig_has_hidden_argc_param(sig)
        || !args.iter().any(|arg| match &arg.kind {
            ExprKind::Spread(source) => {
                hidden || preserve_values
                    || array_literal_element_type_for_ir(ctx, source).codegen_repr() == PhpType::Mixed
                    || nested_spread_assoc_literal(source)
            }
            _ => false,
        })
    {
        return None;
    }
    let span = args.first()?.span;
    let plan = crate::types::call_args::plan_call_args(sig, args, span, false, false).ok()?;
    let mut staged = Vec::with_capacity(plan.source_args.len());
    for arg in &plan.source_args {
        staged.push(match &arg.kind {
            ExprKind::Spread(source) => StagedSpreadArgument::Spread(root_value(ctx, source)),
            ExprKind::NamedArg { name, value } => {
                let index = crate::types::call_args::named_param_index(sig, crate::types::call_args::regular_param_count(sig), name)
                    .expect("shared planner validated fixed named argument");
                StagedSpreadArgument::Named(index, root_value(ctx, value))
            }
            _ => StagedSpreadArgument::Positional(root_value(ctx, arg)),
        });
    }
    let mut state = SpreadBindings {
        slots: Vec::new(), filled: Vec::new(),
        next: initialize_slot(ctx, PhpType::Int, &Expr::new(ExprKind::IntLiteral(0), span)),
        named: initialize_slot(ctx, PhpType::Bool, &Expr::new(ExprKind::BoolLiteral(false), span)),
        actual: initialize_slot(ctx, PhpType::Int, &Expr::new(ExprKind::IntLiteral(0), span)),
        collector: hidden.then(|| initialize_owned_slot(ctx, variadic_array_type(sig),
            &Expr::new(ExprKind::ArrayLiteral(if crate::func_args::sig_collects_optional_arg_count(sig) {
                vec![Expr::new(ExprKind::IntLiteral(0), span)]
            } else { Vec::new() }), span))),
        span,
    };
    for _ in 0..crate::types::call_args::regular_param_count(sig) {
        state
            .slots
            .push(initialize_owned_slot(
                ctx,
                PhpType::Mixed,
                &Expr::new(ExprKind::Null, span),
            ));
        state.filled.push(initialize_slot(ctx, PhpType::Bool, &Expr::new(ExprKind::BoolLiteral(false), span)));
    }
    for arg in &staged {
        match arg {
            StagedSpreadArgument::Spread(source) => lower_source(ctx, sig, &state, source),
            StagedSpreadArgument::Named(index, value) => bind_named(ctx, sig, &state, *index, value),
            StagedSpreadArgument::Positional(value) => bind_positional(ctx, &state, value),
        }
    }
    if !hidden {
        let count = ctx.load_local(&state.next, Some(span));
        let max = emit_i64_at_span(ctx, state.slots.len() as i64, span);
        let valid = compare_ints(ctx, count.value, max.value, CmpPredicate::Sle, span);
        require(ctx, valid, "ArgumentCountError", &format!("{builtin}(): Too many arguments for unpacked call"), span);
    }
    for (index, filled) in state.filled.iter().enumerate() {
        if hidden {
            if let Some(Some(default)) = sig.defaults.get(index) {
                let ready = ctx.load_local(filled, Some(span));
                let done = ctx.builder.create_named_block("spread.default.done", Vec::new());
                let missing = ctx.builder.create_named_block("spread.default.missing", Vec::new());
                conditional(ctx, ready.value, done, missing);
                ctx.builder.position_at_end(missing);
                store_expr_into_temp(ctx, &state.slots[index], PhpType::Mixed, default, span);
                branch(ctx, done);
                ctx.builder.position_at_end(done);
                continue;
            }
        }
        if preserve_values && sig.defaults.get(index).is_some_and(Option::is_some) {
            continue;
        }
        let ready = ctx.load_local(filled, Some(span));
        require(ctx, ready, "ArgumentCountError", "Too few arguments for unpacked call", span);
    }
    let mut operands = Vec::with_capacity(state.slots.len());
    for slot in &state.slots {
        let value = ctx.load_local(&slot, Some(span));
        let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
        ctx.builder.set_value_ownership(owned.value, Ownership::Owned);
        operands.push(owned.value);
    }
    if let Some(collector) = &state.collector {
        if crate::func_args::sig_collects_optional_arg_count(sig) {
            let update = Stmt::new(StmtKind::ArrayAssign {
                array: collector.clone(),
                index: Expr::new(ExprKind::IntLiteral(0), span),
                value: Expr::new(ExprKind::Variable(state.actual.clone()), span),
            }, span);
            crate::ir_lower::stmt::lower_stmt(ctx, &update);
        }
        let value = ctx.load_local(collector, Some(span));
        let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
        ctx.builder.set_value_ownership(owned.value, Ownership::Owned);
        operands.push(owned.value);
        retire_slot(ctx, collector, span);
    }
    for slot in state.slots.into_iter().rev() {
        retire_slot(ctx, &slot, span);
    }
    for arg in staged.into_iter().rev() {
        let source = match arg {
            StagedSpreadArgument::Spread(source)
            | StagedSpreadArgument::Named(_, source)
            | StagedSpreadArgument::Positional(source) => source,
        };
        retire_slot(ctx, &source, span);
    }
    Some(if preserve_values { operands } else { coerce_operands_to_params(ctx, sig, operands) })
}

/// Returns true for an unpack source that is a keyed literal carrying a nested `...$source`.
///
/// The parser files that shape as `ExprKind::ArrayLiteralMixed`. The nested source contributes
/// keys that are unknown until the literal is built, so the static named-argument expansion
/// declines the whole unpack and the signature-ordered lowerings below it only ever see one
/// opaque argument. The runtime walk is therefore the only lowering that can bind this shape,
/// and it does so after evaluating the literal exactly once.
fn nested_spread_assoc_literal(source: &Expr) -> bool {
    matches!(source.kind, ExprKind::ArrayLiteralMixed(_))
}

/// Stores an expression in a rooted slot whose boxed layout does not depend on its current value.
fn root_value(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> String {
    initialize_owned_slot(ctx, PhpType::Mixed, expr)
}

/// Creates and publishes a refcounted temporary for same-frame exception cleanup.
fn initialize_owned_slot(
    ctx: &mut LoweringContext<'_, '_>,
    ty: PhpType,
    expr: &Expr,
) -> String {
    let name = initialize_slot(ctx, ty, expr);
    register_owned_call_operand(ctx, ctx.local_slots[&name], expr.span);
    name
}

/// Creates a compiler-private local and initializes it using ordinary retaining-store semantics.
fn initialize_slot(ctx: &mut LoweringContext<'_, '_>, ty: PhpType, expr: &Expr) -> String {
    let slot = ctx.declare_hidden_temp(ty.clone());
    store_expr_into_temp(ctx, &slot, ty, expr, expr.span);
    slot
}

/// Retires one published temporary after all consumers have acquired their own references.
fn retire_slot(ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span) {
    retire_owned_call_operand(ctx, ctx.local_slots[slot], span);
}

/// Iterates a boxed source with the normal runtime iterator, preserving sparse and named keys.
fn lower_source(ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, state: &SpreadBindings, source_slot: &str) {
    let source = ctx.load_local(source_slot, Some(state.span));
    let (iterator, iterator_owner, _) = ctx.emit_iter_start(source, false, state.span);
    let key_slot = initialize_owned_slot(
        ctx,
        PhpType::Mixed,
        &Expr::new(ExprKind::Null, state.span),
    );
    let value_slot = initialize_owned_slot(
        ctx,
        PhpType::Mixed,
        &Expr::new(ExprKind::Null, state.span),
    );
    let header = ctx.builder.create_named_block("spread.iter.next", Vec::new());
    let body = ctx.builder.create_named_block("spread.iter.body", Vec::new());
    let exit = ctx.builder.create_named_block("spread.iter.exit", Vec::new());
    branch(ctx, header);
    ctx.builder.position_at_end(header);
    let next = ctx.emit_value(Op::IterNext, vec![iterator.value], None, PhpType::Bool,
        Op::IterNext.default_effects(), Some(state.span));
    conditional(ctx, next.value, body, exit);
    ctx.builder.position_at_end(body);
    for (op, slot) in [(Op::IterCurrentKey, &key_slot), (Op::IterCurrentValue, &value_slot)] {
        let value = ctx.emit_value(op, vec![iterator.value], None, PhpType::Mixed,
            op.default_effects(), Some(state.span));
        ctx.store_local(slot, value, PhpType::Mixed, Some(state.span));
    }
    let key = Expr::new(ExprKind::Variable(key_slot.clone()), state.span);
    let is_string = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("is_string"), args: vec![key.clone()],
    }, state.span);
    let is_string = lower_expr(ctx, &is_string);
    let named = ctx.builder.create_named_block("spread.key.named", Vec::new());
    let positional = ctx.builder.create_named_block("spread.key.positional", Vec::new());
    let done = ctx.builder.create_named_block("spread.key.done", Vec::new());
    conditional(ctx, is_string.value, named, positional);
    ctx.builder.position_at_end(positional);
    bind_positional(ctx, state, &value_slot);
    branch(ctx, done);
    ctx.builder.position_at_end(named);
    for (index, (name, _)) in sig.params.iter().take(state.slots.len()).enumerate() {
        let matches = Expr::new(ExprKind::BinaryOp {
            left: Box::new(key.clone()), op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::StringLiteral(name.clone()), state.span)),
        }, state.span);
        let matches = lower_expr(ctx, &matches);
        let matched = ctx.builder.create_named_block("spread.name.match", Vec::new());
        let other = ctx.builder.create_named_block("spread.name.next", Vec::new());
        conditional(ctx, matches.value, matched, other);
        ctx.builder.position_at_end(matched);
        bind_named(ctx, sig, state, index, &value_slot);
        branch(ctx, done);
        ctx.builder.position_at_end(other);
    }
    if state.collector.is_some() {
        let message = Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::StringLiteral("Unknown named parameter $".to_string()), state.span)),
            op: BinOp::Concat, right: Box::new(key),
        }, state.span);
        throw_expression(ctx, "Error", message, state.span);
    } else {
        throw(ctx, "Error", "Unknown named parameter in unpacked call", state.span);
    }
    ctx.builder.position_at_end(done);
    // Keep the slot layouts stable across the loop backedge. The next iteration's
    // stores release these values, and the exit path retires the final pair.
    branch(ctx, header);
    ctx.builder.position_at_end(exit);
    retire_slot(ctx, &value_slot, state.span);
    retire_slot(ctx, &key_slot, state.span);
    if let Some(slot) = iterator_owner {
        ctx.retire_iter_start_owner(slot, state.span);
    }
}

/// Binds a string-keyed value after checking that the parameter has not already been populated.
fn bind_named(ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, state: &SpreadBindings, index: usize, value: &str) {
    let occupied = ctx.load_local(&state.filled[index], Some(state.span));
    let zero = emit_i64_at_span(ctx, 0, state.span);
    let free = compare_ints(ctx, occupied.value, zero.value, CmpPredicate::Eq, state.span);
    let message = if state.collector.is_some() {
        format!("Named parameter ${} overwrites previous argument", sig.params[index].0)
    } else { "Named parameter overwrites previous argument".to_string() };
    require(ctx, free, "Error", &message, state.span);
    bind_slot(ctx, state, index, value);
    let extent = emit_i64_at_span(ctx, index as i64 + 1, state.span);
    extend_actual_count(ctx, state, extent);
    set_flag(ctx, &state.named, state.span);
}

/// Binds by iteration order rather than numeric key and counts surplus positional arguments.
fn bind_positional(ctx: &mut LoweringContext<'_, '_>, state: &SpreadBindings, value: &str) {
    let named = ctx.load_local(&state.named, Some(state.span));
    let zero = emit_i64_at_span(ctx, 0, state.span);
    let allowed = compare_ints(ctx, named.value, zero.value, CmpPredicate::Eq, state.span);
    require(ctx, allowed, "Error", "Cannot use positional argument after named argument during unpacking", state.span);
    let next = ctx.load_local(&state.next, Some(state.span));
    let done = ctx.builder.create_named_block("spread.position.done", Vec::new());
    for index in 0..state.slots.len() {
        let candidate = emit_i64_at_span(ctx, index as i64, state.span);
        let matches = compare_ints(ctx, next.value, candidate.value, CmpPredicate::Eq, state.span);
        let matched = ctx.builder.create_named_block("spread.position.match", Vec::new());
        let other = ctx.builder.create_named_block("spread.position.next", Vec::new());
        conditional(ctx, matches.value, matched, other);
        ctx.builder.position_at_end(matched);
        bind_slot(ctx, state, index, value);
        branch(ctx, done);
        ctx.builder.position_at_end(other);
    }
    if let Some(collector) = &state.collector {
        let array = ctx.load_local(collector, Some(state.span));
        let value = ctx.load_local(value, Some(state.span));
        ctx.emit_void(Op::ArrayPush, vec![array.value, value.value], None,
            Op::ArrayPush.default_effects(), Some(state.span));
    }
    branch(ctx, done);
    ctx.builder.position_at_end(done);
    let one = emit_i64_at_span(ctx, 1, state.span);
    let incremented = ctx.emit_value(Op::IAdd, vec![next.value, one.value], None, PhpType::Int,
        Op::IAdd.default_effects(), Some(state.span));
    ctx.store_local(&state.next, incremented, PhpType::Int, Some(state.span));
    extend_actual_count(ctx, state, incremented);
}

/// Tracks PHP's argument extent, including default-filled gaps before named parameters.
fn extend_actual_count(ctx: &mut LoweringContext<'_, '_>, state: &SpreadBindings, extent: LoweredValue) {
    if state.collector.is_none() { return; }
    let current = ctx.load_local(&state.actual, Some(state.span));
    let greater = compare_ints(ctx, extent.value, current.value, CmpPredicate::Sgt, state.span);
    let update = ctx.builder.create_named_block("spread.count.update", Vec::new());
    let done = ctx.builder.create_named_block("spread.count.done", Vec::new());
    conditional(ctx, greater.value, update, done);
    ctx.builder.position_at_end(update);
    ctx.store_local(&state.actual, extent, PhpType::Int, Some(state.span));
    branch(ctx, done);
    ctx.builder.position_at_end(done);
}

/// Copies the value into its parameter root and marks that parameter as supplied.
fn bind_slot(ctx: &mut LoweringContext<'_, '_>, state: &SpreadBindings, index: usize, value: &str) {
    let value = ctx.load_local(value, Some(state.span));
    store_value_into_temp(ctx, &state.slots[index], PhpType::Mixed, value, state.span);
    set_flag(ctx, &state.filled[index], state.span);
}

/// Marks a boolean bookkeeping slot without affecting user-visible type state.
fn set_flag(ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span) {
    store_expr_into_temp(ctx, slot, PhpType::Bool, &Expr::new(ExprKind::BoolLiteral(true), span), span);
}

/// Compares scalar bookkeeping values with a typed EIR predicate.
fn compare_ints(ctx: &mut LoweringContext<'_, '_>, left: ValueId, right: ValueId, predicate: CmpPredicate, span: Span) -> LoweredValue {
    ctx.emit_value(Op::ICmp, vec![left, right], Some(Immediate::CmpPredicate(predicate)),
        PhpType::Bool, Op::ICmp.default_effects(), Some(span))
}

/// Throws a catchable PHP exception when a runtime argument constraint is false.
fn require(ctx: &mut LoweringContext<'_, '_>, valid: LoweredValue, class: &str, message: &str, span: Span) {
    let ok = ctx.builder.create_named_block("spread.guard.ok", Vec::new());
    let invalid = ctx.builder.create_named_block("spread.guard.invalid", Vec::new());
    conditional(ctx, valid.value, ok, invalid);
    ctx.builder.position_at_end(invalid);
    throw(ctx, class, message, span);
    ctx.builder.position_at_end(ok);
}

/// Emits a PHP exception through the ordinary object construction and throw path.
fn throw(ctx: &mut LoweringContext<'_, '_>, class: &str, message: &str, span: Span) {
    throw_expression(ctx, class, Expr::new(ExprKind::StringLiteral(message.to_string()), span), span);
}

/// Constructs a catchable binding error whose message includes a runtime argument name.
fn throw_expression(ctx: &mut LoweringContext<'_, '_>, class: &str, message: Expr, span: Span) {
    let exception = Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified(class),
        args: vec![message],
    }, span);
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}

/// Connects a completed materialization branch to its continuation.
fn branch(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
}

/// Splits materialization control flow using a boolean EIR value.
fn conditional(ctx: &mut LoweringContext<'_, '_>, cond: ValueId, then_target: BlockId, else_target: BlockId) {
    ctx.builder.terminate(Terminator::CondBr {
        cond, then_target, then_args: Vec::new(), else_target, else_args: Vec::new(),
    });
}
