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
    terminate_branch(
        ctx,
        frame.break_block,
        loop_cleanup_count_for_branch(level),
        level,
        LoopExitKind::Break,
    );
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
        level,
        LoopExitKind::Continue,
    );
}

/// Kind of loop edge used to decide which lexical try handlers the edge exits.
#[derive(Clone, Copy)]
enum LoopExitKind {
    Break,
    Continue,
}

/// Lowers a return statement using the current function return contract.
pub(super) fn lower_return(ctx: &mut LoweringContext<'_, '_>, value_expr: Option<&Expr>, span: Span) {
    // A by-reference-returning function hands the caller the ref-cell pointer of the
    // returned property (`function &f() { return $obj->prop; }`), so `$x = &f()` aliases
    // it. The cell pointer is materialized as the declared return type so the ABI return
    // convention matches the caller's expectation for pointer-sized property types.
    if ctx.by_ref_return {
        if let Some(Expr { kind: ExprKind::PropertyAccess { object, property }, .. }) = value_expr {
            let object = lower_expr(ctx, object);
            if ctx.builder.insertion_block_is_terminated() {
                return;
            }
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
            terminate_return(ctx, Some(cell_ptr.value));
            return;
        }
    }
    if ctx.return_type == IrType::Void {
        if let Some(value_expr) = value_expr {
            lower_expr(ctx, value_expr);
            if ctx.builder.insertion_block_is_terminated() {
                return;
            }
        }
        terminate_return(ctx, None);
        return;
    }
    let value = if let Some(value_expr) = value_expr {
        lower_return_expr(ctx, value_expr)
    } else {
        emit_null_value(ctx, Some(span))
    };
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let value = coerce_to_return_type(ctx, value, Some(span));
    let value = acquire_borrowed_return_value(ctx, value, span);
    let value = acquire_returned_this(ctx, value_expr, value, span);
    let value = persist_scratch_return_string(ctx, value, span);
    terminate_return(ctx, Some(value.value));
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
pub(super) fn persist_scratch_return_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if value.ir_type != IrType::Str {
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
    let handler_count = ctx.try_handler_stack.len();
    terminate_return_with_handlers(ctx, value, handler_count);
}

/// Terminates a return while unwinding lexical catch handlers in nesting order with finalizers.
fn terminate_return_with_handlers(
    ctx: &mut LoweringContext<'_, '_>,
    value: Option<crate::ir::ValueId>,
    handler_count: usize,
) {
    let handler_count = emit_try_handler_pops_before_finally(ctx, handler_count, |_| true);
    if run_innermost_finally(ctx, false) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_return_with_handlers(ctx, value, handler_count);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, ctx.loop_stack.len());
    ctx.emit_eval_scope_finalizer(None);
    ctx.builder.terminate(Terminator::Return { value });
}

/// Terminates with a branch after running active finally bodies from inner to outer.
fn terminate_branch(
    ctx: &mut LoweringContext<'_, '_>,
    target: BlockId,
    loop_cleanup_count: usize,
    level: usize,
    kind: LoopExitKind,
) {
    let handler_count = ctx.try_handler_stack.len();
    terminate_branch_with_handlers(
        ctx,
        target,
        loop_cleanup_count,
        level,
        kind,
        handler_count,
    );
}

/// Terminates a loop edge while unwinding only the catch handlers that edge leaves.
fn terminate_branch_with_handlers(
    ctx: &mut LoweringContext<'_, '_>,
    target: BlockId,
    loop_cleanup_count: usize,
    level: usize,
    kind: LoopExitKind,
    handler_count: usize,
) {
    let loop_depth = ctx.loop_stack.len();
    let remaining_loop_depth = loop_depth.saturating_sub(level);
    let target_loop_depth = remaining_loop_depth.saturating_add(1);
    let handler_count = emit_try_handler_pops_before_finally(ctx, handler_count, |frame| {
        match kind {
            LoopExitKind::Break => frame.loop_depth > remaining_loop_depth,
            LoopExitKind::Continue => frame.loop_depth >= target_loop_depth,
        }
    });
    if run_innermost_finally(ctx, false) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_branch_with_handlers(
                ctx,
                target,
                loop_cleanup_count,
                level,
                kind,
                handler_count,
            );
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, loop_cleanup_count);
    ctx.builder.terminate(Terminator::Br {
        target,
        args: Vec::new(),
    });
}

/// Pops the innermost exited catch handlers that are nested inside the next active finalizer.
fn emit_try_handler_pops_before_finally(
    ctx: &mut LoweringContext<'_, '_>,
    handler_count: usize,
    exits_handler: impl Fn(&TryHandlerFrame) -> bool,
) -> usize {
    let finally_depth = ctx.finally_stack.len();
    let mut remaining = handler_count.min(ctx.try_handler_stack.len());
    let mut popped = Vec::new();
    while remaining > 0 {
        let frame = ctx.try_handler_stack[remaining - 1];
        if frame.finally_depth < finally_depth || !exits_handler(&frame) {
            break;
        }
        popped.push(frame);
        remaining -= 1;
    }
    for frame in popped {
        emit_try_pop_handler(ctx, frame.handler_token, frame.span);
    }
    remaining
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
