//! Purpose:
//! Materializes required, fixed-arity builtin calls whose unpacked PHP arrays have boxed storage.
//!
//! Called from:
//! - Builtin signature-based argument lowering, including statically selected CUF/FCC targets.
//!
//! Key details:
//! - Consumes the shared source plan, then binds runtime keys without discarding named arguments.
//! - Iterator sources and argument values stay in rooted slots across later evaluation and throws.

use super::*;

/// Tracks runtime parameter occupancy separately from the next positional argument index.
struct SpreadBindings {
    slots: Vec<String>,
    filled: Vec<String>,
    next: String,
    named: String,
    span: Span,
}

/// Lowers boxed unpack sources for required signatures without reference, optional or hidden parameters.
pub(super) fn lower_boxed_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    builtin: &str,
) -> Option<Vec<ValueId>> {
    if sig.variadic.is_some() || sig.ref_params.iter().any(|by_ref| *by_ref)
        || sig.defaults.iter().any(Option::is_some)
        || crate::func_args::sig_has_hidden_argc_param(sig)
        || !args.iter().any(|arg| matches!(&arg.kind, ExprKind::Spread(source)
            if array_literal_element_type_for_ir(ctx, source).codegen_repr() == PhpType::Mixed))
    {
        return None;
    }
    let span = args.first()?.span;
    let plan = crate::types::call_args::plan_call_args(sig, args, span, false, false).ok()?;
    let mut state = SpreadBindings {
        slots: Vec::new(), filled: Vec::new(),
        next: initialize_slot(ctx, PhpType::Int, &Expr::new(ExprKind::IntLiteral(0), span)),
        named: initialize_slot(ctx, PhpType::Bool, &Expr::new(ExprKind::BoolLiteral(false), span)),
        span,
    };
    for _ in &sig.params {
        state.slots.push(initialize_slot(ctx, PhpType::Mixed, &Expr::new(ExprKind::Null, span)));
        state.filled.push(initialize_slot(ctx, PhpType::Bool, &Expr::new(ExprKind::BoolLiteral(false), span)));
    }
    let mut sources = Vec::new();
    for arg in &plan.source_args {
        match &arg.kind {
            ExprKind::Spread(source) => sources.push(lower_source(ctx, sig, &state, source)),
            ExprKind::NamedArg { name, value } => {
                let value = root_value(ctx, value);
                let index = crate::types::call_args::named_param_index(sig, sig.params.len(), name)
                    .expect("shared planner validated fixed named argument");
                bind_named(ctx, &state, index, &value);
                clear_slot(ctx, &value, span);
            }
            _ => {
                let value = root_value(ctx, arg);
                bind_positional(ctx, &state, &value);
                clear_slot(ctx, &value, span);
            }
        }
    }
    let count = ctx.load_local(&state.next, Some(span));
    let max = emit_i64_at_span(ctx, sig.params.len() as i64, span);
    let valid = compare_ints(ctx, count.value, max.value, CmpPredicate::Sle, span);
    require(ctx, valid, "ArgumentCountError", &format!("{builtin}(): Too many arguments for unpacked call"), span);
    for filled in &state.filled {
        let ready = ctx.load_local(filled, Some(span));
        require(ctx, ready, "ArgumentCountError", "Too few arguments for unpacked call", span);
    }
    // An invalid surplus value can own an object with a PHP destructor. Keep
    // every source alive until all source effects and arity checks have run.
    for source in sources {
        clear_slot(ctx, &source, span);
    }
    let mut operands = Vec::with_capacity(state.slots.len());
    for slot in state.slots {
        let value = ctx.load_local(&slot, Some(span));
        let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
        ctx.builder.set_value_ownership(owned.value, Ownership::Owned);
        operands.push(owned.value);
        clear_slot(ctx, &slot, span);
    }
    Some(operands)
}

/// Stores an expression in a rooted slot whose boxed layout does not depend on its current value.
fn root_value(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> String {
    initialize_slot(ctx, PhpType::Mixed, expr)
}

/// Creates a compiler-private local and initializes it using ordinary retaining-store semantics.
fn initialize_slot(ctx: &mut LoweringContext<'_, '_>, ty: PhpType, expr: &Expr) -> String {
    let slot = ctx.declare_hidden_temp(ty.clone());
    store_expr_into_temp(ctx, &slot, ty, expr, expr.span);
    slot
}

/// Retires one rooted temporary after all consumers have acquired their own references.
fn clear_slot(ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span) {
    let null = lower_null(ctx, &Expr::new(ExprKind::Null, span));
    ctx.unset_local(slot, null, Some(span));
}

/// Iterates a boxed source with the normal runtime iterator, preserving sparse and named keys.
fn lower_source(ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, state: &SpreadBindings, source: &Expr) -> String {
    let source_slot = root_value(ctx, source);
    let source = ctx.load_local(&source_slot, Some(state.span));
    let (iterator, iterator_owner) = ctx.emit_iter_start(source, false, state.span);
    let key_slot = initialize_slot(ctx, PhpType::Mixed, &Expr::new(ExprKind::Null, state.span));
    let value_slot = initialize_slot(ctx, PhpType::Mixed, &Expr::new(ExprKind::Null, state.span));
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
    for (index, (name, _)) in sig.params.iter().enumerate() {
        let matches = Expr::new(ExprKind::BinaryOp {
            left: Box::new(key.clone()), op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::StringLiteral(name.clone()), state.span)),
        }, state.span);
        let matches = lower_expr(ctx, &matches);
        let matched = ctx.builder.create_named_block("spread.name.match", Vec::new());
        let other = ctx.builder.create_named_block("spread.name.next", Vec::new());
        conditional(ctx, matches.value, matched, other);
        ctx.builder.position_at_end(matched);
        bind_named(ctx, state, index, &value_slot);
        branch(ctx, done);
        ctx.builder.position_at_end(other);
    }
    throw(ctx, "Error", "Unknown named parameter in unpacked call", state.span);
    ctx.builder.position_at_end(done);
    // Keep the slot layouts stable across the loop backedge. The next iteration's
    // stores release these values, and the exit path retires the final pair.
    branch(ctx, header);
    ctx.builder.position_at_end(exit);
    if let Some(slot) = iterator_owner {
        ctx.retire_iter_start_owner(slot, state.span);
    }
    clear_slot(ctx, &key_slot, state.span);
    clear_slot(ctx, &value_slot, state.span);
    source_slot
}

/// Binds a string-keyed value after checking that the parameter has not already been populated.
fn bind_named(ctx: &mut LoweringContext<'_, '_>, state: &SpreadBindings, index: usize, value: &str) {
    let occupied = ctx.load_local(&state.filled[index], Some(state.span));
    let zero = emit_i64_at_span(ctx, 0, state.span);
    let free = compare_ints(ctx, occupied.value, zero.value, CmpPredicate::Eq, state.span);
    require(ctx, free, "Error", "Named parameter overwrites previous argument", state.span);
    bind_slot(ctx, state, index, value);
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
    branch(ctx, done);
    ctx.builder.position_at_end(done);
    let one = emit_i64_at_span(ctx, 1, state.span);
    let incremented = ctx.emit_value(Op::IAdd, vec![next.value, one.value], None, PhpType::Int,
        Op::IAdd.default_effects(), Some(state.span));
    ctx.store_local(&state.next, incremented, PhpType::Int, Some(state.span));
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
    let exception = Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified(class),
        args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
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
