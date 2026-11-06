//! Purpose:
//! Lowers array_splice arguments, alias snapshots, and receiver widening.
//!
//! Called from:
//! - `crate::ir_lower::expr` builtin argument lowering.
//!
//! Key details:
//! - Snapshots precede receiver widening and preserve the pre-splice replacement value.
//! - Ref-cell alias checks over-approximate sharing without changing PHP evaluation order.

use super::*;

/// Lowers `array_splice()` operands, promoting a typed receiver whose `$replacement` cannot fit.
///
/// PHP has no per-array element type, so `$a = [1, 2, 3]; array_splice($a, 1, 1, ["x"])` simply
/// leaves `[1, "x", 3]`. elephc types an indexed array at its payload slot, so the promotion has
/// to reach the receiver LOCAL: `__rt_array_to_mixed` re-boxes every live payload and the slot's
/// storage type widens to `array<mixed>`, which is the representation the boxed insert helper
/// writes into. Without it the backend would have to store a string pointer/length pair in an
/// 8-byte integer slot, which is why the untyped case used to be an explicit `unsupported`
/// diagnostic instead of a wrong answer.
pub(super) fn lower_array_splice_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let mut operands = if crate::types::call_args::has_named_args(args) {
        lower_args_with_signature(ctx, sig, args)
    } else {
        lower_positional_builtin_args_with_signature(ctx, sig, args)
    };
    materialize_boxed_array_splice_replacement(ctx, args, &mut operands);
    snapshot_array_splice_self_replacement(ctx, &mut operands);
    widen_array_splice_receiver_for_replacement(ctx, sig, args, &mut operands);
    operands
}

/// Materializes a BOXED `$replacement` into the fresh `array<mixed>` the splice inserts from.
///
/// A declared `array` value is boxed here -- a `public array $items` property, an `array &$r`
/// parameter, a nested element, a `&$x` binding -- so `array_splice($a, 1, 1, $box->items)`
/// handed the backend a `Mixed` cell where every insert helper needs an indexed payload, and
/// the call was refused as an unsupported replacement type. Unpacking the cell into a hash
/// (`ArrayUnpackToHash` reads a packed or a hash payload alike, boxing each value) and taking
/// that hash's values yields the `array<mixed>` the `Mixed`-receiver path inserts verbatim.
///
/// The copy is also what PHP evaluates: `$replacement` is read into its own array BEFORE the
/// receiver is touched, so a replacement aliasing the receiver (`array_splice($box->items, 1,
/// 1, $box->items)`) inserts the pre-splice contents (issue #676) without the storage
/// comparison `snapshot_array_splice_self_replacement` needs for plain locals, which cannot see
/// through two reads of one property cell.
fn materialize_boxed_array_splice_replacement(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    operands: &mut [crate::ir::ValueId],
) {
    let Some(&replacement) = operands.get(3) else {
        return;
    };
    // A declared `array` value, or a plain `mixed` one such as a static `array` property (which
    // is typed `mixed` rather than as the array union). A `mixed` box that turns out to hold a
    // scalar raises the unpack helper's `Only arrays and Traversables can be unpacked` Error
    // rather than PHP's `(array)` cast: a loud stop where the backend used to refuse the call
    // at compile time. Unions (`?array`) keep that refusal, since `null` inserts nothing in PHP.
    let replacement_ty = ctx.builder.value_php_type(replacement);
    if !replacement_ty.is_php_array() && replacement_ty != PhpType::Mixed {
        return;
    }
    let span = ctx
        .builder
        .value_defining_instruction(replacement)
        .and_then(|inst| inst.span)
        .or_else(|| args.first().map(|arg| arg.span))
        .unwrap_or_else(Span::dummy);
    let boxed = LoweredValue {
        value: replacement,
        ir_type: IrType::Heap(IrHeapKind::Mixed),
    };
    let unpacked = indexed_array_literals::lower_boxed_array_spread_source(ctx, boxed, span);
    let values = function_calls::emit_builtin_call_value(
        ctx,
        "array_values",
        vec![unpacked.value],
        PhpType::Array(Box::new(PhpType::Mixed)),
        span,
        None,
    );
    // The hash was only the stepping stone to the indexed copy; the copy itself is an owning
    // operand the call lowering releases after the splice, like any other temporary argument.
    crate::ir_lower::ownership::release_if_owned(ctx, unpacked, Some(span));
    operands[3] = values.value;
}

/// Copies `$replacement` before the splice runs when it IS the receiver.
///
/// PHP evaluates `$replacement` into its own array before touching the receiver, so
/// `array_splice($a, 1, 1, $a)` inserts what `$a` held on the way in. elephc passed the receiver
/// pointer twice and the insertion then read slots the removal had already overwritten:
/// `[1, 2, 3]` came back as `[1, 1, 1, 3]` where PHP gives `[1, 1, 2, 3, 3]` (issue #676).
///
/// Decided on the LOWERED OPERANDS, which are already in parameter order, so every spelling is
/// covered by one test: positional, named in any order, and a named `$replacement` with
/// `$length` left out. Matching the source arguments instead would have to redo the planner's
/// parameter mapping, and getting it wrong is silent - the wrong answer simply comes back.
///
/// Two loads of the same local slot are the same array, so the comparison is exact and needs no
/// runtime check; a call that cannot alias pays nothing. It runs before
/// `widen_array_splice_receiver_for_replacement` so the copy holds the receiver's original
/// payload rather than a re-boxed version of it, and unlike that pass it accepts every receiver
/// shape - a by-ref parameter and a `&$x` binding alias just as hard, and neither needs the slot
/// retyping that forces the other pass to turn them away.
///
/// A replacement reaching the same array WITHOUT loading a slot the receiver also loads - out of
/// a container the receiver is also stored in, say - still aliases. Deciding that needs the
/// heap pointers, which are not available until the call runs.
fn snapshot_array_splice_self_replacement(
    ctx: &mut LoweringContext<'_, '_>,
    operands: &mut [crate::ir::ValueId],
) {
    let Some(&receiver) = operands.first() else {
        return;
    };
    let Some(&replacement) = operands.get(3) else {
        return;
    };
    // Both operands have to BE arrays for a clone to mean anything, and the check has to happen
    // before the storage comparison rather than after it. A reference rebound to a scalar keeps
    // its slot (`$b = &$a; $b = &$x;` where `$x` is an int), and the earlier alias instruction
    // stays in the function, so the alias walk below still reports a possible match; cloning that
    // integer emitted an `ArrayCloneShallow` the EIR validator rejects outright.
    if !matches!(
        ctx.builder.value_php_type(receiver).codegen_repr(),
        PhpType::Array(_)
    ) || !matches!(
        ctx.builder.value_php_type(replacement).codegen_repr(),
        PhpType::Array(_)
    ) {
        return;
    }
    let Some(receiver_storage) = loaded_slot(ctx, receiver) else {
        return;
    };
    let Some(replacement_storage) = loaded_slot(ctx, replacement) else {
        return;
    };
    if !storages_may_be_the_same(ctx, receiver_storage, replacement_storage) {
        return;
    }
    let replacement_ty = ctx.builder.value_php_type(replacement);
    let span = ctx
        .builder
        .value_defining_instruction(replacement)
        .and_then(|inst| inst.span);
    let snapshot = ctx.emit_value(
        Op::ArrayCloneShallow,
        vec![replacement],
        None,
        replacement_ty,
        Op::ArrayCloneShallow.default_effects(),
        span,
    );
    operands[3] = snapshot.value;
}

/// Reports whether two loaded storages can be the same array.
///
/// Identical storage is the common case. Two DIFFERENT ref-cell slots still share one cell when
/// a `&$x` binding aliased them (`$b = &$a` emits `alias_local_ref_cell slots[b, a]`), so the
/// function's alias instructions are consulted before giving up, transitively - `$c = &$b` after
/// `$b = &$a` puts all three on one cell.
///
/// Deliberately an OVER-approximation: an alias inside a branch not taken at runtime still counts
/// here, and so does one the source later rebound away. That is safe in the only direction that
/// matters, because the sole consequence of a false positive is one extra array copy, while a
/// false negative is the wrong answer - but it is only safe once the CALLER has established that
/// both operands are arrays, since a rebound reference can leave a scalar behind the same slot.
fn storages_may_be_the_same(
    ctx: &LoweringContext<'_, '_>,
    left: (Op, LocalSlotId),
    right: (Op, LocalSlotId),
) -> bool {
    if left == right {
        return true;
    }
    let ((left_op, left_slot), (right_op, right_slot)) = (left, right);
    if left_op != Op::LoadRefCell || right_op != Op::LoadRefCell {
        return false;
    }
    let mut reachable = HashSet::from([left_slot]);
    // One pass per edge is enough to close the relation: each pass adds at least one slot, or
    // nothing is left to add.
    for _ in 0..ctx.builder.function().instructions.len() {
        let mut grew = false;
        for inst in &ctx.builder.function().instructions {
            if inst.op != Op::AliasLocalRefCell {
                continue;
            }
            let Some(Immediate::LocalSlotPair { first, second }) = inst.immediate else {
                continue;
            };
            if reachable.contains(&first) && reachable.insert(second) {
                grew = true;
            }
            if reachable.contains(&second) && reachable.insert(first) {
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    reachable.contains(&right_slot)
}

/// Returns the storage a value was loaded from, as the `(load opcode, slot)` pair.
///
/// `LoadRefCell` counts beside `LoadLocal` so a by-reference parameter is covered: that is where
/// the receiver reaches the CALLER's array, and two reads of one cell are as much the same array
/// as two reads of one local. The opcode is part of the key rather than discarded, so two
/// different kinds of storage can never compare equal on a shared slot number.
///
/// Anything else - a literal, a call result, a converted value - reports `None`, because only a
/// load identifies storage two operands can share.
fn loaded_slot(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> Option<(Op, LocalSlotId)> {
    let inst = ctx.builder.value_defining_instruction(value)?;
    if !matches!(inst.op, Op::LoadLocal | Op::LoadRefCell) {
        return None;
    }
    match inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Some((inst.op, slot)),
        _ => None,
    }
}

/// Promotes an `array_splice()` receiver local to `array<mixed>` when `$replacement` retypes it.
///
/// Runs after the operands are lowered because the decision needs both the receiver's slot
/// element type and the replacement's EIR type, and the conversion itself re-reads the receiver
/// local so it observes any mutation the later arguments performed.
fn widen_array_splice_receiver_for_replacement(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    operands: &mut [crate::ir::ValueId],
) {
    let Some(sig) = sig else {
        return;
    };
    let Some(replacement) = operands.get(3).copied() else {
        return;
    };
    let Some((name, span)) = array_splice_receiver_local(ctx, sig, args) else {
        return;
    };
    let PhpType::Array(elem_ty) = ctx.local_type(&name).codegen_repr() else {
        return;
    };
    let elem_ty = elem_ty.codegen_repr();
    if elem_ty == PhpType::Mixed {
        return;
    }
    let replacement_ty = ctx.builder.value_php_type(replacement).codegen_repr();
    if array_splice_replacement_fits_receiver(&elem_ty, &replacement_ty) {
        return;
    }
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let local = ctx.load_local(&name, Some(span));
    let converted = ctx.emit_value(
        Op::ArrayToMixed,
        vec![local.value],
        None,
        array_ty.clone(),
        Op::ArrayToMixed.default_effects(),
        Some(span),
    );
    ctx.store_mutated_local(&name, converted, array_ty, Some(span));
    operands[0] = ctx.load_local(&name, Some(span)).value;
}

/// Returns the expression `array_splice()`'s by-reference receiver argument was written as.
///
/// Positional and named spellings both land on parameter 0, so callers see one shape whichever
/// way the call was written.
fn array_splice_receiver_place<'a>(sig: &FunctionSig, args: &'a [Expr]) -> Option<&'a Expr> {
    args.iter().enumerate().find_map(|(index, arg)| {
        let (param_index, place) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (
                sig.params.iter().position(|(param, _)| param == name)?,
                value.as_ref(),
            ),
            _ => (index, arg),
        };
        (param_index == 0).then_some(place)
    })
}

/// Returns the plain local variable bound to `array_splice()`'s by-reference receiver.
///
/// Two receiver shapes are deliberately excluded even though they name a local. A by-reference
/// parameter and a `&$x` binding share storage with a caller slot this function cannot retype,
/// and the hidden `__eir_place` temporary of the property/element rewrite is written back into a
/// place whose declared element type is equally out of reach. Widening either would publish
/// boxed `Mixed` cells through a slot still described as `array<int>`, so both keep the
/// backend's explicit diagnostic instead.
fn array_splice_receiver_local(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<(String, Span)> {
    let receiver = array_splice_receiver_place(sig, args)?;
    let ExprKind::Variable(name) = &receiver.kind else {
        return None;
    };
    if !ctx.has_local_slot(name) || ctx.is_ref_bound_local(name) {
        return None;
    }
    if name.starts_with(crate::names::SYNTHETIC_PLACE_LOCAL_STEM) {
        return None;
    }
    Some((name.clone(), receiver.span))
}

/// Reports whether a `$replacement` can be written into the receiver's existing payload slots.
///
/// Mirrors the shapes the backend's `SpliceReplacement` classifier accepts, so a call this
/// predicate passes never reaches the `unsupported` arm: an omitted/null/empty replacement
/// inserts nothing, an array of the receiver's own element type is copied verbatim, an array of
/// boxed `Mixed` cells is read back as plain integers for an `int`/`bool` receiver (the shape
/// `[$x + 1]` produces), and a bare scalar of the element type becomes a one-element insertion.
fn array_splice_replacement_fits_receiver(elem_ty: &PhpType, replacement_ty: &PhpType) -> bool {
    if matches!(replacement_ty, PhpType::Void | PhpType::Never) {
        return true;
    }
    if let PhpType::Array(inner) = replacement_ty {
        let inner = inner.codegen_repr();
        if matches!(inner, PhpType::Void | PhpType::Never) {
            return true;
        }
        if &inner == elem_ty {
            return true;
        }
        return inner == PhpType::Mixed && matches!(elem_ty, PhpType::Int | PhpType::Bool);
    }
    replacement_ty == elem_ty
}
