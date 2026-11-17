//! Purpose:
//! Yield and yield-from expression lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `yield`.
pub(super) fn lower_yield(ctx: &mut LoweringContext<'_, '_>, key: Option<&Expr>, value: Option<&Expr>, expr: &Expr) -> LoweredValue {
    let mut operands = Vec::new();
    let mut key_owner = if let Some(key) = key {
        let key = lower_expr(ctx, key);
        let (key, owner) = root_owned_call_operand(ctx, key, expr.span);
        operands.push(key.value);
        owner
    } else {
        None
    };
    let yielded = if let Some(value) = value {
        let source = lower_expr(ctx, value);
        // Transfer a fresh value into one suspension owner, retiring the producer now.
        let yielded = if ctx.value_is_owning_temporary(source) {
            let boxed = ctx.box_value_as_mixed(source, PhpType::Mixed, Some(expr.span));
            let source_is_marked_copy = ctx.builder.value_defining_instruction(source.value)
                .is_some_and(|inst| inst.op == Op::StrPersist
                    && inst.immediate == Some(Immediate::Bool(true)));
            if !source_is_marked_copy {
                // The boxed result owns its bytes; nested normalization copies are no longer read.
                crate::ir_lower::ownership::release_marked_normalized_cast_copies(ctx, source, Some(expr.span));
            }
            boxed
        } else { source };
        operands.push(yielded.value);
        Some(yielded)
    } else { None };
    if key_owner.is_some() && ctx.builder.value_php_type(operands[0]).codegen_repr() == PhpType::Str {
        let owner = key_owner.take().unwrap();
        // The key's box must own its bytes before its source owner retires. Transfer this
        // single-use box to the yield runtime so no new owner remains on a suspended stack.
        let key = ctx.emit_value(
            Op::MixedBox, vec![operands[0]], None, PhpType::Mixed,
            Op::MixedBox.default_effects(), Some(expr.span),
        );
        retire_owned_call_operand(ctx, owner, expr.span);
        operands[0] = key.value;
    }
    let result = ctx.emit_value(Op::GeneratorYield, operands, None, PhpType::Mixed,
        Op::GeneratorYield.default_effects(), Some(expr.span));
    if let Some(owner) = key_owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    }
    // The yield boxes its own copy of the string. The cast copy under `trim`
    // is not that cell and would otherwise stay live for every suspension.
    if let Some(yielded) = yielded {
        crate::ir_lower::ownership::release_marked_normalized_cast_copies(
            ctx, yielded, Some(expr.span),
        );
    }
    result
}

/// Lowers `yield from`.
///
/// `yield from <generator|Traversable>` lowers to `Op::GeneratorYieldFrom`, which
/// the backend delegates to `__rt_gen_delegate` (forwarding sent values and
/// producing the inner generator's return value). `yield from <array>` is
/// desugared here into an iterator loop that re-yields each key/value pair,
/// reusing the foreach iterator opcodes; its result is PHP null.
pub(super) fn lower_yield_from(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let runtime_class = matches!(source_ty, PhpType::Iterable | PhpType::Mixed | PhpType::Union(_))
        || matches!(&source_ty, PhpType::Object(name)
            if name.trim_start_matches('\\') != "Generator"
                && ctx.interfaces.contains_key(name.trim_start_matches('\\')));
    if runtime_class {
        return lower_yield_from_runtime_class(ctx, value, expr);
    }
    if matches!(source_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        || matches!(&source_ty, PhpType::Object(name) if name.trim_start_matches('\\') != "Generator")
    {
        return lower_yield_from_array(ctx, value, expr);
    }
    lower_yield_from_generator(ctx, value, expr)
}

/// Lowers `yield from` over a value whose class is known only at run time (`mixed`, `iterable`,
/// a union, an interface): a `Generator` is delegated to, so its return value comes back as the
/// expression's result; anything else is iterated and re-yielded, giving null.
fn lower_yield_from_runtime_class(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let mut source = source;
    if ctx.builder.value_php_type(source.value).codegen_repr() == PhpType::Iterable {
        // An `iterable` holds a bare heap pointer whose array or object nature is known only
        // to the boxer: box it so the tag test below can tell an array from a Generator.
        let original = source;
        let held = crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span));
        source = ctx.box_value_as_mixed(held, PhpType::Mixed, Some(span));
        if ctx.value_is_owning_temporary(original) {
            crate::ir_lower::ownership::release_if_owned(ctx, original, Some(span));
        }
    }
    let source_ty = ctx.builder.value_php_type(source.value).codegen_repr();
    let boxed_source = matches!(source_ty, PhpType::Mixed | PhpType::Union(_));
    let check_class = ctx.builder.create_named_block("yieldfrom.check", Vec::new());
    let delegate = ctx.builder.create_named_block("yieldfrom.delegate", Vec::new());
    let iterate = ctx.builder.create_named_block("yieldfrom.iterate", Vec::new());
    let merge = ctx.builder.create_named_block("yieldfrom.result", Vec::new());
    if boxed_source {
        // The tag test first, so an array never reaches the class test.
        let is_object = ctx.emit_value(
            Op::TypePredicate,
            vec![source.value],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
            PhpType::Bool,
            Op::TypePredicate.default_effects(),
            Some(span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: is_object.value,
            then_target: check_class,
            then_args: Vec::new(),
            else_target: iterate,
            else_args: Vec::new(),
        });
    } else {
        ctx.builder.terminate(Terminator::Br { target: check_class, args: Vec::new() });
    }
    let split = ctx.initialized_slots_snapshot();

    ctx.builder.position_at_end(check_class);
    let generator = ctx.intern_class_name("Generator");
    let is_generator = ctx.emit_value(
        Op::InstanceOf,
        vec![source.value],
        Some(Immediate::Data(generator)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_generator.value,
        then_target: delegate,
        then_args: Vec::new(),
        else_target: iterate,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(delegate);
    ctx.restore_initialized_slots(split.clone());
    let target = PhpType::Object("Generator".to_string());
    let inner = if boxed_source {
        ctx.emit_owned_value(
            Op::MixedUnbox,
            vec![source.value],
            None,
            target.clone(),
            Op::mixed_unbox_effects(&target),
            Some(span),
        )
    } else if source_ty == PhpType::Iterable {
        // An `iterable` slot already holds the unboxed object pointer.
        source
    } else {
        crate::ir_lower::stmt::borrow_receiver_as_runtime_class(ctx, source, "Generator", span)
    };
    let passthrough = inner.value == source.value;
    let result = lower_yield_from_generator(ctx, inner, expr);
    if !passthrough && ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    store_value_into_temp(ctx, &temp, PhpType::Mixed, result, span);
    let merged = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::Br { target: merge, args: Vec::new() });

    ctx.builder.position_at_end(iterate);
    ctx.restore_initialized_slots(split);
    let result = lower_yield_from_array(ctx, source, expr);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, result, span);
    ctx.builder.terminate(Terminator::Br { target: merge, args: Vec::new() });

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merged);
    take_owned_temp(ctx, &temp, span)
}

/// Delegates `yield from` to a value known to be a `Generator`, returning its return value.
fn lower_yield_from_generator(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let staging = prepublish_user_call_result(
        ctx, None, &ReturnArgAlias::None, &PhpType::Mixed, expr.span,
    );
    let (value, owner) = root_owned_call_operand(ctx, value, expr.span);
    let result = ctx.emit_value(
        Op::GeneratorYieldFrom,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::GeneratorYieldFrom.default_effects(),
        Some(expr.span),
    );
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    // Delegation can suspend or throw before returning. Its temporary owner must
    // unwind before a same-frame finally, while borrowed locals keep their owner.
    if let Some(owner) = owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    }
    take_prepublished_call_result(ctx, staging, result, expr.span)
}

/// Desugars `yield from <array|Traversable>` into an iterator loop that re-yields
/// each key/value pair, returning a boxed PHP null (non-Generator iterables have
/// no delegated return value). Reuses the foreach iterator opcodes so every
/// supported iterable representation is handled by existing iterator lowering.
pub(super) fn lower_yield_from_array(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    // An `IteratorAggregate` source needs an owning slot for the iterator `getIterator()`
    // returns, exactly as a `foreach` over one does (`lower_foreach`).
    let (iterator, iterator_owner, _iterator_state) = ctx.emit_iter_start(source, false, span);
    let header = ctx.builder.create_named_block("yieldfrom.next", Vec::new());
    let body = ctx.builder.create_named_block("yieldfrom.body", Vec::new());
    let exit = ctx.builder.create_named_block("yieldfrom.exit", Vec::new());
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target: header,
            args: Vec::new(),
        });
    }

    ctx.builder.position_at_end(header);
    let has_next = ctx.emit_value(
        Op::IterNext,
        vec![iterator.value],
        None,
        PhpType::Bool,
        Op::IterNext.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let key = ctx.emit_value(
        Op::IterCurrentKey,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentKey.default_effects(),
        Some(span),
    );
    let element = ctx.emit_value(
        Op::IterCurrentValue,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentValue.default_effects(),
        Some(span),
    );
    // Re-yield the inner key/value pair through the outer generator. The sent
    // value is discarded (arrays ignore it). The `Immediate::Bool(true)` marks
    // the yield as *delegated*: PHP forwards `yield from` keys verbatim, so an
    // integer key from the inner array must not advance the outer generator's
    // implicit-key counter (`yield from [7 => "x"]` leaves it where it was).
    ctx.emit_value(
        Op::GeneratorYield,
        vec![key.value, element.value],
        Some(Immediate::Bool(true)),
        PhpType::Mixed,
        Op::GeneratorYield.default_effects(),
        Some(span),
    );
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target: header,
            args: Vec::new(),
        });
    }

    ctx.builder.position_at_end(exit);
    if let Some(owner) = iterator_owner {
        ctx.retire_iter_start_owner(owner, span);
    }
    // The iterator borrows a freshly-created array (e.g. a literal): release it
    // once iteration ends, mirroring `lower_foreach`.
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    let null_value = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            Some(span),
        )
        .expect("const_null produces a value");
    // A fresh null is non-refcounted: there is no producer reference to release,
    // so this boxes directly rather than via box_value_as_mixed (issue #484).
    ctx.emit_value(
        Op::MixedBox,
        vec![null_value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}
