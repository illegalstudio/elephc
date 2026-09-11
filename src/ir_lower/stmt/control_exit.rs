//! Purpose:
//! Break, continue, return, throw exits, and finally cleanup.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a `break` terminator.
pub(super) fn lower_break(ctx: &mut LoweringContext<'_, '_>, level: usize) {
    let Some(frame) = loop_target(ctx, level) else {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    };
    terminate_branch(ctx, frame.break_block, loop_cleanup_count_for_branch(level));
}

/// Lowers a `continue` terminator.
pub(super) fn lower_continue(ctx: &mut LoweringContext<'_, '_>, level: usize) {
    let Some(frame) = loop_target(ctx, level) else {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    };
    terminate_branch(
        ctx,
        frame.continue_block,
        loop_cleanup_count_for_branch(level),
    );
}

/// Lowers a return statement using the current function return contract.
pub(super) fn lower_return(ctx: &mut LoweringContext<'_, '_>, value_expr: Option<&Expr>, span: Span) {
    // A by-reference-returning function hands the caller the ref-cell pointer of the
    // returned place (`function &f() { return $obj->prop; }`), so `$x = &f()` aliases
    // it. Metadata retains the place's declared type for caller dereferencing;
    // the reference-return ABI always transports the raw cell in the integer result register.
    if ctx.by_ref_return && ctx.return_type != IrType::Void {
        lower_reference_return(ctx, value_expr, span);
        return;
    }
    if ctx.return_type == IrType::Void {
        if let Some(value_expr) = value_expr {
            lower_expr(ctx, value_expr);
        }
        terminate_return(ctx, None);
        return;
    }
    let value = if let Some(value_expr) = value_expr {
        lower_return_expr(ctx, value_expr)
    } else {
        emit_null_value(ctx, Some(span))
    };
    let value = coerce_to_return_type(ctx, value, Some(span));
    let value = acquire_borrowed_return_value(ctx, value, span);
    let value = acquire_returned_this(ctx, value_expr, value, span);
    let value = persist_scratch_return_string(ctx, value, span);
    terminate_return(ctx, Some(value.value));
}

/// Lowers the value of a by-reference `return`, which must transport a managed cell.
///
/// The reference-return ABI always places a raw cell pointer in the integer result register, so
/// every accepted source has to OWN a transferable cell. Two shapes qualify: a reference-bound
/// local whose cell this frame can address, and an object property whose slot already holds a
/// promoted cell. An ordinary addressable local is promoted in place first, which preserves its
/// identity (later writes through the variable are seen through the caller's alias).
///
/// Everything else is refused with a compile diagnostic rather than lowered as a value, because
/// a value-shaped return would put a payload word where the caller expects an address. These are
/// SUBSET limits, not PHP semantics: PHP happily returns a reference to an array element or to
/// the result of another reference-returning call, and this compiler simply has no way yet to
/// transfer an owning cell for those places.
///
/// The refusals here are early diagnostics for provenance this frame can see. Provenance it
/// cannot see, such as an alias relayed in through a by-reference parameter, is caught by
/// the owner-zero guard in `codegen::lower_inst::local_stores::lower_acquire_ref_cell`, which
/// raises a catchable `Error` instead of publishing an interior address. Its one exception is
/// an EXACT active `array_walk()` element borrow, whose caller is the descriptor invoker and
/// copies the pointee before any cleanup runs.
fn lower_reference_return(
    ctx: &mut LoweringContext<'_, '_>,
    value_expr: Option<&Expr>,
    span: Span,
) {
    match value_expr.map(|expr| &expr.kind) {
        Some(ExprKind::Variable(name)) => {
            if ctx.is_borrowed_element_ref_local(name) {
                refuse_reference_return(
                    ctx,
                    span,
                    "Unsupported by-reference return: this compiler cannot transfer an alias \
                     of an array element out of its frame, because that address lies inside the \
                     array's payload and owns no reference cell of its own. PHP supports the \
                     return; this lowering has no cell to hand the caller",
                );
                return;
            }
            if !ctx.is_ref_bound_local(name) {
                if !ctx.local_is_promotable_to_ref_cell(name) {
                    refuse_reference_return(
                        ctx,
                        span,
                        "Unsupported by-reference return: this compiler can transfer only a \
                         local it can promote to a managed reference cell in place, which a \
                         global, a static local, an extern global and an eval-scope name are \
                         not",
                    );
                    return;
                }
                promote_local_to_reference_return_payload(ctx, name, span);
            }
            if !reference_return_payload_matches(ctx, name) {
                refuse_reference_return(
                    ctx,
                    span,
                    "Unsupported by-reference return: this variable's reference cell stores a \
                     different payload representation than the declared by-reference result, so \
                     the caller would read the aliased storage with the wrong shape",
                );
                return;
            }
            let value = ctx.load_local(name, Some(span));
            if ctx.builder.value_defining_op(value.value) != Some(Op::LoadRefCell) {
                refuse_reference_return(
                    ctx,
                    span,
                    "Unsupported by-reference return: this variable is not backed by a managed \
                     reference cell on every path reaching the return",
                );
                return;
            }
            acquire_and_return_reference_cell(ctx, value, span);
        }
        Some(ExprKind::PropertyAccess { object, property }) => {
            let object = lower_expr(ctx, object);
            let data = ctx.intern_string(property);
            let result_ty = ctx.return_php_type.clone();
            let cell_ptr = ctx.emit_value(
                Op::LoadPropRefCell,
                vec![object.value],
                Some(Immediate::Data(data)),
                result_ty,
                Op::LoadPropRefCell.default_effects(),
                Some(span),
            );
            let owning_receiver = ctx.value_is_owning_temporary(object);
            let captured = acquire_reference_return_owner(ctx, cell_ptr, span);
            if owning_receiver {
                crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
            }
            terminate_return(ctx, Some(captured.value));
        }
        Some(_) => {
            if let Some(value_expr) = value_expr {
                // Keep the source expression's side effects even though its value cannot be
                // returned; the program is refused, so only the diagnostic is observable.
                lower_expr(ctx, value_expr);
            }
            refuse_reference_return(
                ctx,
                span,
                "Unsupported by-reference return: this compiler transfers a reference only \
                 from a variable or a property. PHP also allows other places here, such as an \
                 array element or another reference-returning call, but this lowering cannot \
                 transfer an owning cell for them yet",
            );
        }
        None => refuse_reference_return(
            ctx,
            span,
            "Unsupported by-reference return: a by-reference function with a declared result \
             must return a reference",
        ),
    }
}

/// Records an unsupported by-reference return and terminates the block without a value.
///
/// `Terminator::Unreachable` keeps the lowered function well formed without inventing a cell
/// pointer; the recorded refusal makes `lower_program` fail before the module reaches codegen.
fn refuse_reference_return(ctx: &mut LoweringContext<'_, '_>, span: Span, message: &str) {
    crate::ir_lower::diagnostics::refuse(span, message);
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Promotes an ordinary local to a cell whose payload matches the declared by-reference result.
///
/// The caller dereferences the transferred cell with the representation of the callee's DECLARED
/// result, so a local whose inferred storage is narrower than that (a concretely typed array
/// local inside a `: array` function, whose declared payload representation is `Mixed`) has to be
/// widened BEFORE the cell is created. Widening afterwards would rewrite the local's storage
/// without the caller's alias following it, and returning the narrow cell would make the caller
/// read the aliased storage with the wrong shape.
fn promote_local_to_reference_return_payload(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    span: Span,
) {
    if ctx.return_php_type.codegen_repr() == PhpType::Mixed
        && ctx.local_type(name).codegen_repr() != PhpType::Mixed
    {
        ctx.promote_local_mixed_ref_cell(name, Some(span));
        return;
    }
    ctx.promote_local_ref_cell(name, Some(span));
}

/// Returns whether a local's cell payload agrees with the declared by-reference result shape.
///
/// A by-reference parameter aliases storage this frame does not own, so it cannot be widened:
/// when its payload representation disagrees with the declared result, the only sound outcome is
/// a refusal. Alias writes through the returned reference keep their meaning precisely because
/// both sides now agree on one representation.
fn reference_return_payload_matches(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    ctx.local_type(name).reference_payload_compatible(&ctx.return_php_type)
}

/// Retains the returned cell in the frame's lease slot and yields the address it captured.
///
/// `AcquireRefCell` materializes the cell address ONCE and publishes it as a typed `Pointer`
/// SSA result, which the return terminator transports. Reading that snapshot, instead of
/// rematerializing the returned variable's CURRENT cell, is what keeps the acquisition and the
/// returned reference identical when a fallthrough `finally` rebinds that variable in between
/// (PHP snapshots the same way, with `MAKE_REF` before the finally).
///
/// The `ReturnRefCell` slot it also writes is only the optional managed LEASE: it holds the
/// retained owner so cleanup can retire it, and a superseded lease left by an earlier return is
/// retired after the replacement is published, because that retirement can run a throwing
/// destructor. An accepted address that owns no managed cell, which is the active
/// `array_walk()` element borrow the descriptor invoker copies out immediately, publishes a
/// zero lease and is still returned through the snapshot.
fn acquire_reference_return_owner(
    ctx: &mut LoweringContext<'_, '_>,
    cell_ptr: LoweredValue,
    span: Span,
) -> LoweredValue {
    let owner = ctx.declare_local_with_kind(
        "__eir_reference_return_owner",
        PhpType::Pointer(None),
        crate::ir::LocalKind::ReturnRefCell,
    );
    ctx.emit_value(
        Op::AcquireRefCell,
        vec![cell_ptr.value],
        Some(Immediate::LocalSlot(owner)),
        PhpType::Pointer(None),
        Op::AcquireRefCell.default_effects(),
        Some(span),
    )
}

/// Acquires the reference-return lease and terminates with the address it captured.
fn acquire_and_return_reference_cell(
    ctx: &mut LoweringContext<'_, '_>,
    cell_ptr: LoweredValue,
    span: Span,
) {
    let captured = acquire_reference_return_owner(ctx, cell_ptr, span);
    terminate_return(ctx, Some(captured.value));
}

/// Lowers a return expression with contextual array-literal element storage when available.
pub(super) fn lower_return_expr(ctx: &mut LoweringContext<'_, '_>, value_expr: &Expr) -> LoweredValue {
    if matches!(value_expr.kind, ExprKind::ArrayLiteral(_)) {
        if let PhpType::Array(elem_ty) = ctx.return_php_type.codegen_repr() {
            return lower_array_literal_with_expected_type(ctx, value_expr, *elem_ty);
        }
    }
    lower_expr(ctx, value_expr)
}

/// Acquires the receiver when a method does `return $this`.
///
/// `$this` is a borrowed reference to the receiver the caller still owns. A return
/// value is handed to the caller as owned, so without an extra reference the
/// caller's release of the (often discarded, as in fluent `$obj->setX(...)->setY()`)
/// result drops the object's refcount to zero and runs its destructor while the
/// original binding is still live — a use-after-free for any class with a
/// destructor. Incrementing the refcount here balances that release.
pub(super) fn acquire_returned_this(
    ctx: &mut LoweringContext<'_, '_>,
    value_expr: Option<&Expr>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !matches!(value_expr.map(|expr| &expr.kind), Some(ExprKind::This)) {
        return value;
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
}

/// Copies scratch-backed string results before they cross a function boundary.
/// Already-owned results transfer directly instead of leaking an unnecessary duplicate.
pub(super) fn persist_scratch_return_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if value.ir_type != IrType::Str
        || ctx.builder.value_ownership(value.value) == Ownership::Owned
    {
        return value;
    }
    let Some(op) = ctx.builder.value_defining_op(value.value) else {
        return value;
    };
    if !string_op_uses_scratch_storage(op) {
        return value;
    }
    ctx.emit_value(
        Op::StrPersist,
        vec![value.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(span),
    )
}

/// Acquires return values read from heap containers before local cleanup runs.
///
/// Function-static slots are included: the slot keeps owning its boxed value across
/// calls, so `return $static_local` must hand the caller an extra reference — the
/// caller releases call results after consuming them, and without the retain that
/// release frees the box the slot still points to.
pub(super) fn acquire_borrowed_return_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.value_is_owning_temporary(value) {
        return value;
    }
    let php_type = ctx.builder.value_php_type(value.value);
    if !Ownership::php_type_needs_lifetime_tracking(&php_type) {
        return value;
    }
    if !ctx.by_ref_return && ctx.builder.value_defining_op(value.value) == Some(Op::LoadRefCell) {
        // A by-value return cannot transfer the caller's mutable reference owner.
        // Detach Mixed cells so native reference writeback cannot invalidate the result.
        if php_type.codegen_repr() == PhpType::Mixed {
            return ctx.emit_owned_value(
                Op::MixedClone, vec![value.value], None, php_type,
                Op::MixedClone.default_effects(), Some(span),
            );
        }
        return crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    }
    if !matches!(
        ctx.builder.value_defining_op(value.value),
        Some(
            Op::ArrayGet
                | Op::HashGet
                | Op::HashGetSilent
                | Op::PropGet
                | Op::DynamicPropGet
                | Op::NullsafePropGet
                | Op::LoadStaticLocal
        )
    ) {
        return value;
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
}

/// Terminates with a return after running active finally bodies from inner to outer.
pub(super) fn terminate_return(ctx: &mut LoweringContext<'_, '_>, value: Option<crate::ir::ValueId>) {
    if run_innermost_finally(ctx, false) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_return(ctx, value);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, ctx.loop_stack.len());
    ctx.emit_eval_scope_finalizer(None);
    ctx.builder.terminate(Terminator::Return { value });
}

/// Terminates with a branch after running active finally bodies from inner to outer.
pub(super) fn terminate_branch(ctx: &mut LoweringContext<'_, '_>, target: BlockId, loop_cleanup_count: usize) {
    if run_innermost_finally(ctx, false) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_branch(ctx, target, loop_cleanup_count);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, loop_cleanup_count);
    ctx.builder.terminate(Terminator::Br {
        target,
        args: Vec::new(),
    });
}

/// Terminates with a throw after running finally bodies that apply to uncaught throws.
pub(super) fn terminate_throw(ctx: &mut LoweringContext<'_, '_>, value: crate::ir::ValueId) {
    if run_innermost_finally(ctx, true) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_throw(ctx, value);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, ctx.loop_stack.len());
    ctx.builder.terminate(Terminator::Throw { value });
}

/// Lowers a statically-decided access violation as a catchable `Error` throw.
///
/// Builds a synthetic `new Error($message)` expression at `span`, lowers it to an
/// EIR object value, then terminates the current block with a throw. Mirrors PHP,
/// which raises these conditions as catchable `Error` exceptions instead of fatal
/// compile-time rejections. Used in statement positions where no value is needed.
pub(crate) fn lower_throw_access_error(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
        },
        span,
    );
    let error_value = crate::ir_lower::expr::lower_expr(ctx, &error_expr);
    terminate_throw(ctx, error_value.value);
}

/// Lowers a statically-decided access violation as a catchable `Error` throw in
/// expression position and returns a placeholder null value.
///
/// Builds a synthetic `new Error($message)` expression at `span`, lowers it to an
/// EIR object value, emits `Op::ThrowException`, then returns a null placeholder so
/// the surrounding expression lowering keeps producing well-formed EIR after the
/// (unreachable) throw.
pub(crate) fn lower_throw_access_error_expr(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
) -> LoweredValue {
    let error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
        },
        span,
    );
    let error_value = crate::ir_lower::expr::lower_expr(ctx, &error_expr);
    ctx.emit_void(
        Op::ThrowException,
        vec![error_value.value],
        None,
        Op::ThrowException.default_effects(),
        Some(span),
    );
    LoweredValue {
        value: ctx
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
            .expect("const_null produces a value"),
        ir_type: IrType::I64,
    }
}

/// Returns how many inner loop cleanups a multi-level branch skips.
pub(super) fn loop_cleanup_count_for_branch(level: usize) -> usize {
    level.max(1).saturating_sub(1)
}

/// Emits cleanup for the innermost active loops that will not reach their exit block.
pub(super) fn emit_innermost_loop_cleanups(ctx: &mut LoweringContext<'_, '_>, count: usize) {
    let frames = ctx
        .loop_stack
        .iter()
        .rev()
        .take(count)
        .copied()
        .collect::<Vec<_>>();
    for frame in frames {
        if let Some(cleanup) = frame.cleanup {
            crate::ir_lower::ownership::release_if_owned(ctx, cleanup.value, Some(cleanup.span));
        }
        // A by-reference `foreach` over an element source holds a lifetime reference on the
        // element for the whole loop; leaving through `break N`, `return`, or `throw` never
        // reaches the exit block that would drop it, so drop it here (issue #580).
        if let Some(pin) = frame.source_pin {
            crate::ir_lower::ownership::release_if_owned(ctx, pin.value, Some(pin.span));
        }
    }
}

/// Runs and removes the innermost applicable finally frame.
pub(super) fn run_innermost_finally(ctx: &mut LoweringContext<'_, '_>, is_throw: bool) -> bool {
    let Some(frame) = ctx.finally_stack.last() else {
        return false;
    };
    if is_throw && !frame.run_on_throw {
        return false;
    }
    let frame = ctx
        .finally_stack
        .pop()
        .expect("finally frame disappeared after last() check");
    if let Some((handler_token, span)) = frame.handler_cleanup {
        emit_try_pop_handler(ctx, handler_token, span);
    }
    lower_block(ctx, &frame.body);
    true
}

/// Pushes a finalizer and returns the stack depth before the push.
pub(super) fn push_finally_frame(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
    run_on_throw: bool,
    handler_cleanup: Option<(i64, Span)>,
) -> usize {
    let depth = ctx.finally_stack.len();
    ctx.finally_stack.push(FinallyFrame {
        body: body.to_vec(),
        run_on_throw,
        handler_cleanup,
    });
    depth
}

/// Removes a finalizer when the protected body fell through normally.
pub(super) fn pop_finally_frame_if_active(ctx: &mut LoweringContext<'_, '_>, depth: usize) {
    if ctx.finally_stack.len() > depth {
        ctx.finally_stack.pop();
    }
}
