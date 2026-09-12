//! Purpose:
//! PHP argument-unpacking key semantics for signature-unknown callable-descriptor containers.
//!
//! Called from:
//! - `super::descriptor_calls` and `super::descriptor_args`, for every explicit argument and
//!   every `...$source` written into a descriptor hash container.
//!
//! Key details:
//! - A declared PHP `array` is `Union([Array(Mixed), AssocArray])`, whose `codegen_repr()` is
//!   `Mixed`, so its storage is `Heap(Mixed)`. `Op::ArrayLen`/`Op::ArrayGet` reject that receiver
//!   in `crate::ir::validator`. The walk uses the ordinary runtime iterator for indexed,
//!   hash, boxed and supported Traversable sources, preserving insertion order.
//! - Integer keys renumber onto the destination's next positional index; string keys keep their
//!   name. A duplicate name is rejected BEFORE the destination is modified, so no destructor for
//!   a replaced entry can run for a binding the call never performs.
//! - The write and the duplicate probe go through `Op::DescriptorArgSet` and
//!   `Op::DescriptorArgKeyExists`, not `Op::HashSet` and `array_key_exists`. A descriptor
//!   container keys parameter NAMES: a Traversable that yields the string key `"12"` is passing
//!   `$12` by name, and PHP's array-key normalization would silently turn it into position 12.
//!   Ordinary PHP array writes are untouched and keep normalizing.
//! - The named-seen flag spans every source and every explicit argument of one container, which
//!   is what makes an integer key after a name a runtime `Error` like PHP.
//! - The evaluated source is published as a call-operand owner for the whole walk and retired at
//!   the exit: a guard that throws mid-walk cannot strand it, and a later argument that throws
//!   cannot observe it still alive.
//! - `Op::IterStart` produces a stack cursor. It is never rooted and never released.
//! - A Mixed owner for `getIterator()` is published on the same source/key/value LIFO
//!   stack immediately before `IterStart` and retired first at the exit.
//! - Every guard rejection leaves through a direct `Terminator::Throw` (or, for the duplicate
//!   name, `Op::ThrowNamedParameterOverwrite` then `Terminator::Unreachable`) rather than
//!   `crate::ir_lower::stmt::terminate_throw`. That is deliberate, and it is safe under exactly
//!   one invariant: every heap value this walk owns at a rejection point is published as a
//!   call-operand owner record (the destination container, the pinned source, and the key and
//!   value slots). Every rejection after iterator setup retires the innermost `getIterator()`
//!   owner locally before terminating, while BOTH rejection forms reach `__rt_throw_current`, whose
//!   `__rt_exception_cleanup_frames` retires those records innermost-first before the handler
//!   resumes. `terminate_throw` cannot be used here: it emits the ENCLOSING loop frames'
//!   `PopCallOperandOwner` cleanups, and at a rejection point this walk's own records sit on top
//!   of that stack, so those pops would retire the wrong records and break LIFO order.

use super::*;

/// Per-container unpack bookkeeping shared by explicit arguments and every unpacked source.
pub(super) struct DescriptorUnpackState {
    /// Published construction slot holding the destination hash.
    owner: LocalSlotId,
    /// Storage type the destination is reloaded with after each insertion.
    hash_ty: PhpType,
    /// Hidden integer slot holding the next positional key.
    next_index: String,
    /// Hidden boolean slot set once any named key has been bound.
    named_seen: String,
}

/// Opens unpack bookkeeping for one already published descriptor hash container.
pub(super) fn begin_descriptor_unpack(
    ctx: &mut LoweringContext<'_, '_>,
    owner: LocalSlotId,
    hash_ty: PhpType,
    span: Span,
) -> DescriptorUnpackState {
    let next_index = ctx.declare_hidden_temp(PhpType::Int);
    store_expr_into_temp(
        ctx,
        &next_index,
        PhpType::Int,
        &Expr::new(ExprKind::IntLiteral(0), span),
        span,
    );
    let named_seen = ctx.declare_hidden_temp(PhpType::Bool);
    store_expr_into_temp(
        ctx,
        &named_seen,
        PhpType::Bool,
        &Expr::new(ExprKind::BoolLiteral(false), span),
        span,
    );
    DescriptorUnpackState { owner, hash_ty, next_index, named_seen }
}

/// Binds one positional value at the container's next integer key.
pub(super) fn bind_descriptor_unpack_positional(
    ctx: &mut LoweringContext<'_, '_>,
    state: &DescriptorUnpackState,
    value: LoweredValue,
    iterator_owner: Option<LocalSlotId>,
    span: Span,
) {
    let (value, owner) = root_descriptor_binding_value(ctx, value, span);
    reject_positional_after_named(ctx, state, iterator_owner, span);
    let key = ctx.load_local(&state.next_index, Some(span));
    insert_descriptor_entry(ctx, state, key, value, span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    let current = ctx.load_local(&state.next_index, Some(span));
    let one = emit_i64_at_span(ctx, 1, span);
    let next = ctx.emit_value(
        Op::IAdd,
        vec![current.value, one.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    ctx.store_local(&state.next_index, next, PhpType::Int, Some(span));
}

/// Binds one named value, rejecting a name the container already carries.
pub(super) fn bind_descriptor_unpack_named(
    ctx: &mut LoweringContext<'_, '_>,
    state: &DescriptorUnpackState,
    key: LoweredValue,
    value: LoweredValue,
    iterator_owner: Option<LocalSlotId>,
    span: Span,
) {
    let (value, owner) = root_descriptor_binding_value(ctx, value, span);
    reject_duplicate_name(ctx, state, key, iterator_owner, span);
    insert_descriptor_entry(ctx, state, key, value, span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    store_expr_into_temp(
        ctx,
        &state.named_seen,
        PhpType::Bool,
        &Expr::new(ExprKind::BoolLiteral(true), span),
        span,
    );
}

/// Unpacks one already evaluated `...$source` into the container with PHP's key rules.
///
/// The source is walked exactly once. Its own reference is published for the whole walk and
/// retired at the exit, so the container is the only owner of the copied entries by the time the
/// next argument expression runs.
pub(super) fn lower_descriptor_unpack_source(
    ctx: &mut LoweringContext<'_, '_>,
    state: &DescriptorUnpackState,
    source: LoweredValue,
    span: Span,
) {
    // A boxed source keeps the guarded, unreachable iterator path codegen-valid even for
    // a statically non-Traversable object or callable. Evaluation still occurs only once.
    let source = coerce_descriptor_invoker_mixed_value(ctx, source, span);
    // Pin even borrowed sources: iterator methods can rebind the caller's original storage.
    let pinned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span));
    let (pinned, source_owner) = root_owned_call_operand(ctx, pinned, span);
    crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    let source = pinned;
    reject_non_iterable_source(ctx, source, span);
    let key_slot = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let value_slot = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    for slot in [&key_slot, &value_slot] {
        // OwnedTemp slots are zero-initialized and stores move their fresh boxed results.
        register_owned_call_operand(ctx, ctx.local_slots[slot], span);
    }
    // Publish the getIterator owner inside the source/key/value LIFO stack, last,
    // so retirement below is exact reverse order.
    let (iterator, iterator_owner) = ctx.emit_iter_start(source, false, span);
    let header = ctx.builder.create_named_block("descriptor.unpack.next", Vec::new());
    let body = ctx.builder.create_named_block("descriptor.unpack.body", Vec::new());
    let exit = ctx.builder.create_named_block("descriptor.unpack.exit", Vec::new());
    branch_to(ctx, header);

    ctx.builder.position_at_end(header);
    let has_entry = ctx.emit_value(
        Op::IterNext,
        vec![iterator.value],
        None,
        PhpType::Bool,
        Op::IterNext.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_entry.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    // The preceding iteration cleared both slots. Publish each fresh result before asking
    // the iterator for another one, since user-defined key/current methods can throw.
    for (op, slot) in [(Op::IterCurrentKey, &key_slot), (Op::IterCurrentValue, &value_slot)] {
        let current = ctx.emit_value(
            op,
            vec![iterator.value],
            None,
            PhpType::Mixed,
            op.default_effects(),
            Some(span),
        );
        ctx.store_local(slot, current, PhpType::Mixed, Some(span));
    }
    let named_key = ctx.builder.create_named_block("descriptor.unpack.key.named", Vec::new());
    let index_check = ctx.builder.create_named_block("descriptor.unpack.key.check", Vec::new());
    let positional_key =
        ctx.builder.create_named_block("descriptor.unpack.key.positional", Vec::new());
    let invalid_key = ctx.builder.create_named_block("descriptor.unpack.key.invalid", Vec::new());
    let entry_done = ctx.builder.create_named_block("descriptor.unpack.entry.done", Vec::new());
    branch_on_key_type(
        ctx,
        &key_slot,
        crate::ir::PhpTypePredicate::String,
        named_key,
        index_check,
        span,
    );

    ctx.builder.position_at_end(index_check);
    branch_on_key_type(
        ctx,
        &key_slot,
        crate::ir::PhpTypePredicate::Int,
        positional_key,
        invalid_key,
        span,
    );

    ctx.builder.position_at_end(positional_key);
    let value = borrow_unpack_entry(ctx, &value_slot, span);
    bind_descriptor_unpack_positional(ctx, state, value, iterator_owner, span);
    branch_to(ctx, entry_done);

    ctx.builder.position_at_end(named_key);
    let key = borrow_unpack_entry(ctx, &key_slot, span);
    let value = borrow_unpack_entry(ctx, &value_slot, span);
    bind_descriptor_unpack_named(ctx, state, key, value, iterator_owner, span);
    branch_to(ctx, entry_done);

    ctx.builder.position_at_end(invalid_key);
    // Retire the innermost getIterator owner before the Throw terminator, leaving
    // source/key/value records published for the ordinary exception unwinder. The guarded
    // positional and duplicate-name rejections above apply the same rule on their throw arms.
    if let Some(slot) = iterator_owner {
        ctx.retire_iter_start_owner(slot, span);
    }
    throw_unpack_rejection(
        ctx,
        UnpackRejection::Fixed("Keys must be of type int|string during argument unpacking"),
        span,
    );

    ctx.builder.position_at_end(entry_done);
    clear_unpack_slot(ctx, &value_slot, span);
    clear_unpack_slot(ctx, &key_slot, span);
    branch_to(ctx, header);

    ctx.builder.position_at_end(exit);
    if let Some(slot) = iterator_owner {
        ctx.retire_iter_start_owner(slot, span);
    }
    retire_owned_call_operand(ctx, ctx.local_slots[&value_slot], span);
    retire_owned_call_operand(ctx, ctx.local_slots[&key_slot], span);
    // `root_owned_call_operand` returns no slot exactly when the source needs no release, so
    // there is nothing left to retire on that path.
    if let Some(slot) = source_owner {
        retire_owned_call_operand(ctx, slot, span);
    }
}

/// Writes one borrowed entry into the published container.
///
/// The container is reloaded from its slot first, because growth reallocates the table and
/// writes the new pointer back into that slot.
fn insert_descriptor_entry(
    ctx: &mut LoweringContext<'_, '_>,
    state: &DescriptorUnpackState,
    key: LoweredValue,
    value: LoweredValue,
    span: Span,
) {
    let hash = load_published_container(ctx, state.owner, state.hash_ty.clone(), span);
    ctx.emit_void(
        Op::DescriptorArgSet,
        vec![hash.value, key.value, value.value],
        None,
        Op::DescriptorArgSet.default_effects(),
        Some(span),
    );
}

/// Keeps a fresh explicit argument alive through guards without transferring its published lease.
/// HashSet retains this borrowed view; retirement then releases only the argument's own lease.
fn root_descriptor_binding_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<LocalSlotId>) {
    // Concrete callable hash payloads use a scalar ABI and do not retain the descriptor.
    // Store a boxed callable so the ordinary Mixed retaining-store contract applies.
    let value = if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Callable {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    } else { value };
    let ty = ctx.builder.value_php_type(value.value);
    let (value, owner) = root_owned_call_operand(ctx, value, span);
    let borrowed = owner.map(|slot| load_published_container(ctx, slot, ty, span)).unwrap_or(value);
    (borrowed, owner)
}

/// Rejects an integer-keyed argument that follows a name, like PHP's unpack ordering rule.
fn reject_positional_after_named(
    ctx: &mut LoweringContext<'_, '_>,
    state: &DescriptorUnpackState,
    iterator_owner: Option<LocalSlotId>,
    span: Span,
) {
    let named = ctx.load_local(&state.named_seen, Some(span));
    let zero = emit_i64_at_span(ctx, 0, span);
    let allowed = ctx.emit_value(
        Op::ICmp,
        vec![named.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    guard_unpack_with_iterator_owner(
        ctx,
        allowed,
        UnpackRejection::Fixed(
            "Cannot use positional argument after named argument during unpacking",
        ),
        iterator_owner,
        span,
    );
}

/// Rejects a name the container already carries, before any entry is overwritten.
///
/// `array_key_exists` rather than `isset` semantics: a name bound to null is still bound, and
/// overwriting it would run the replaced value's destructor for a binding PHP refuses outright.
/// The probe uses the descriptor key space, so it sees the same raw name the write stores.
///
/// The refusal names the key, which only exists at run time when it came from a `Traversable`,
/// so it goes through `Op::ThrowNamedParameterOverwrite` rather than a constructed `Error`.
fn reject_duplicate_name(
    ctx: &mut LoweringContext<'_, '_>,
    state: &DescriptorUnpackState,
    key: LoweredValue,
    iterator_owner: Option<LocalSlotId>,
    span: Span,
) {
    let hash = load_published_container(ctx, state.owner, state.hash_ty.clone(), span);
    let present = ctx.emit_value(
        Op::DescriptorArgKeyExists,
        vec![hash.value, key.value],
        None,
        PhpType::Bool,
        Op::DescriptorArgKeyExists.default_effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    let free = ctx.emit_value(
        Op::ICmp,
        vec![present.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    guard_unpack_with_iterator_owner(
        ctx,
        free,
        UnpackRejection::DuplicateName(key),
        iterator_owner,
        span,
    );
}

/// Rejects a source PHP cannot unpack, before any iteration state is created.
fn reject_non_iterable_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Span,
) {
    let iterable = ctx.emit_value(
        Op::TypePredicate,
        vec![source.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Iterable)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(span),
    );
    guard_unpack(
        ctx,
        iterable,
        UnpackRejection::Fixed("Only arrays and Traversables can be unpacked"),
        span,
    );
}

/// Splits control flow on the runtime PHP type of a rooted key slot.
fn branch_on_key_type(
    ctx: &mut LoweringContext<'_, '_>,
    key_slot: &str,
    predicate: crate::ir::PhpTypePredicate,
    success: BlockId,
    failure: BlockId,
    span: Span,
) {
    let key = borrow_unpack_entry(ctx, key_slot, span);
    let matches = ctx.emit_value(
        Op::TypePredicate,
        vec![key.value],
        Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: matches.value,
        then_target: success,
        then_args: Vec::new(),
        else_target: failure,
        else_args: Vec::new(),
    });
}

/// Reads a rooted unpack slot without moving the reference the slot holds.
///
/// Hash keys are normalized into registers by the backend and never retained, so a key consumer
/// must see a borrow: treating it as a temporary would free the slot's own value.
fn borrow_unpack_entry(
    ctx: &mut LoweringContext<'_, '_>,
    slot: &str,
    span: Span,
) -> LoweredValue {
    let value = ctx.load_local(slot, Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Clears one iteration's value before releasing it, leaving the unwind record published.
fn clear_unpack_slot(ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span) {
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(ctx.local_slots[slot])),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
}

/// What one violated unpack constraint raises.
enum UnpackRejection {
    /// A constraint whose PHP wording is fixed, raised as an ordinary constructed `Error`.
    Fixed(&'static str),
    /// PHP's duplicate-name refusal, whose text names the key the caller actually supplied.
    ///
    /// A `Traversable` decides that name at run time, so the message cannot be built here.
    DuplicateName(LoweredValue),
}

/// Continues on a true unpack constraint and throws a catchable PHP `Error` otherwise.
fn guard_unpack(
    ctx: &mut LoweringContext<'_, '_>,
    valid: LoweredValue,
    rejection: UnpackRejection,
    span: Span,
) {
    let ok = ctx.builder.create_named_block("descriptor.unpack.guard.ok", Vec::new());
    let throw = ctx.builder.create_named_block("descriptor.unpack.guard.throw", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: valid.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: throw,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(throw);
    throw_unpack_rejection(ctx, rejection, span);
    ctx.builder.position_at_end(ok);
}

/// Continues on a true constraint, retiring the innermost iterator owner before rejection.
///
/// Only guards emitted after `IterStart` use this form. The iterator record is at the top of the
/// owner stack, so the rejection branch can retire it without disturbing the source/key/value
/// records that the exception unwinder owns.
fn guard_unpack_with_iterator_owner(
    ctx: &mut LoweringContext<'_, '_>,
    valid: LoweredValue,
    rejection: UnpackRejection,
    iterator_owner: Option<LocalSlotId>,
    span: Span,
) {
    let ok = ctx.builder.create_named_block("descriptor.unpack.guard.ok", Vec::new());
    let throw = ctx.builder.create_named_block("descriptor.unpack.guard.throw", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: valid.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: throw,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(throw);
    if let Some(slot) = iterator_owner {
        ctx.retire_iter_start_owner(slot, span);
    }
    throw_unpack_rejection(ctx, rejection, span);
    ctx.builder.position_at_end(ok);
}

/// Raises one rejection and terminates the block it was emitted into.
///
/// Both arms end in the same runtime unwinder, so both are catchable in the caller's own frame
/// and both let `__rt_exception_cleanup_frames` retire this walk's published owner records.
fn throw_unpack_rejection(
    ctx: &mut LoweringContext<'_, '_>,
    rejection: UnpackRejection,
    span: Span,
) {
    match rejection {
        UnpackRejection::Fixed(message) => throw_unpack_error(ctx, message, span),
        UnpackRejection::DuplicateName(key) => {
            ctx.emit_void(
                Op::ThrowNamedParameterOverwrite,
                vec![key.value],
                None,
                Op::ThrowNamedParameterOverwrite.default_effects(),
                Some(span),
            );
            // `__rt_throw_named_parameter_overwrite` never returns.
            ctx.builder.terminate(Terminator::Unreachable);
        }
    }
}

/// Emits `throw new Error($message)` through the ordinary object construction path.
fn throw_unpack_error(ctx: &mut LoweringContext<'_, '_>, message: &str, span: Span) {
    let exception = lower_expr(
        ctx,
        &Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("Error"),
                args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
            },
            span,
        ),
    );
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}

/// Uses one key-normalizing walk for every spread, including sole-spread calls.
pub(super) fn descriptor_args_need_runtime_unpack_keys(args: &[Expr]) -> bool {
    args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
}
