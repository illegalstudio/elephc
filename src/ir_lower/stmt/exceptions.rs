//! Purpose:
//! Throw, try, catch, and finally CFG lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a throwing statement into a terminator.
pub(super) fn lower_throw(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) {
    let value = lower_expr(ctx, expr);
    // The in-flight exception cell owns one reference to the thrown object. Throwing
    // an owning temporary (e.g. `throw new E()`, `throw f()`) transfers that
    // reference; throwing a value that still leaves a local slot as owner — a
    // PhpLocal/StaticLocal heap load such as a rethrown catch variable (`throw $e`)
    // — must retain it, so the local's own release (rebind or epilogue) stays
    // balanced with the catch-side release of the in-flight reference (issue #448).
    // Main classifies concrete object local loads as owning temporaries for
    // provisional unbox-release tracking; that must not be mistaken for a transfer.
    let transferable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let value = if transferable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(expr.span))
    };
    terminate_throw(ctx, value.value);
}

/// Lowers a `try`/`catch` statement into a runtime handler and explicit catch-dispatch blocks.
pub(super) fn lower_try(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: Option<&[Stmt]>,
    span: Span,
) {
    if let Some(finally_body) = finally_body {
        lower_try_with_finally(ctx, try_body, catches, finally_body, span);
        return;
    }

    lower_try_catch(ctx, try_body, catches, span);
}

/// Lowers a `try`/`catch` statement without a `finally` block.
pub(super) fn lower_try_catch(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    span: Span,
) {
    let handler_block = ctx
        .builder
        .create_named_block("try.catch_dispatch", Vec::new());
    let after_block = ctx.builder.create_named_block("try.after", Vec::new());
    let handler_token = handler_block.as_raw() as i64;
    let mut after_reachable = false;

    ctx.emit_void(
        Op::TryPushHandler,
        Vec::new(),
        Some(Immediate::I64(handler_token)),
        Op::TryPushHandler.default_effects(),
        Some(span),
    );
    // A throw raised inside this body may be caught here, so it does not leave the loops this
    // `try` is nested in; recording the depth is what tells `terminate_throw` which loops it
    // really leaves (issue #690).
    ctx.push_try_loop_depth();
    lower_block(ctx, try_body);
    ctx.pop_try_loop_depth();
    if !ctx.builder.insertion_block_is_terminated() {
        emit_try_pop_handler(ctx, handler_token, span);
        branch_to(ctx, after_block);
        after_reachable = true;
    }

    ctx.builder.position_at_end(handler_block);
    ctx.clear_static_callable_locals();
    emit_try_pop_handler(ctx, handler_token, span);
    after_reachable |= lower_catch_dispatch(ctx, catches, after_block, span);
    ctx.builder.position_at_end(after_block);
    if !after_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
    ctx.clear_static_callable_locals();
}

/// Lowers `try`/`catch`/`finally` using duplicated finalizer bodies for explicit exits.
pub(super) fn lower_try_with_finally(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &[Stmt],
    span: Span,
) {
    if catches.is_empty() {
        lower_try_finally_without_catches(ctx, try_body, finally_body, span);
    } else {
        lower_try_catch_finally(ctx, try_body, catches, finally_body, span);
    }
}

/// Lowers `try`/`finally` with a runtime handler so call-induced throws run the finalizer.
pub(super) fn lower_try_finally_without_catches(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    finally_body: &[Stmt],
    span: Span,
) {
    lower_try_catch_finally(ctx, try_body, &[], finally_body, span);
}

/// Lowers a `try`/`catch`/`finally` statement while preserving catch-before-finally order.
pub(super) fn lower_try_catch_finally(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &[Stmt],
    span: Span,
) {
    let handler_block = ctx
        .builder
        .create_named_block("try.catch_dispatch", Vec::new());
    let after_block = ctx.builder.create_named_block("try.after", Vec::new());
    let handler_token = handler_block.as_raw() as i64;
    let mut after_reachable = false;

    ctx.emit_void(
        Op::TryPushHandler,
        Vec::new(),
        Some(Immediate::I64(handler_token)),
        Op::TryPushHandler.default_effects(),
        Some(span),
    );
    let depth = push_finally_frame(ctx, finally_body, false, Some((handler_token, span)));
    // See the twin in `lower_try_catch`: the handler is active for this body only.
    ctx.push_try_loop_depth();
    lower_block(ctx, try_body);
    ctx.pop_try_loop_depth();
    pop_finally_frame_if_active(ctx, depth);
    if !ctx.builder.insertion_block_is_terminated() {
        emit_try_pop_handler(ctx, handler_token, span);
        lower_block(ctx, finally_body);
        if !ctx.builder.insertion_block_is_terminated() {
            branch_to(ctx, after_block);
            after_reachable = true;
        }
    }

    ctx.builder.position_at_end(handler_block);
    ctx.clear_static_callable_locals();
    emit_try_pop_handler(ctx, handler_token, span);
    after_reachable |=
        lower_catch_dispatch_with_finally(ctx, catches, after_block, finally_body, span);
    ctx.builder.position_at_end(after_block);
    if !after_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
    ctx.clear_static_callable_locals();
}

/// Emits the runtime cleanup for a pushed try/catch handler.
pub(super) fn emit_try_pop_handler(ctx: &mut LoweringContext<'_, '_>, handler_token: i64, span: Span) {
    ctx.emit_void(
        Op::TryPopHandler,
        Vec::new(),
        Some(Immediate::I64(handler_token)),
        Op::TryPopHandler.default_effects(),
        Some(span),
    );
}

/// Lowers ordered catch matching and reports whether any catch reaches the post-try join.
pub(super) fn lower_catch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    catches: &[CatchClause],
    after_block: BlockId,
    span: Span,
) -> bool {
    let mut after_reachable = false;
    for catch in catches {
        let catch_body = ctx.builder.create_named_block("try.catch_body", Vec::new());
        let next_catch = ctx.builder.create_named_block("try.catch_next", Vec::new());
        lower_catch_match(ctx, catch, catch_body, next_catch, span);
        ctx.builder.position_at_end(catch_body);
        lower_catch_bind(ctx, catch, span);
        lower_block(ctx, &catch.body);
        if !ctx.builder.insertion_block_is_terminated() {
            branch_to(ctx, after_block);
            after_reachable = true;
        }
        ctx.clear_static_callable_locals();
        ctx.builder.position_at_end(next_catch);
    }

    // No catch matched, so the exception continues outward. This rethrow leaves every loop
    // the abandoned `try` was nested in -- its record is already popped -- so it releases what
    // the throw into this handler deliberately did not (issue #690).
    let current = lower_current_exception(ctx, span);
    crate::ir_lower::stmt::control_exit::terminate_throw(ctx, current.value);
    after_reachable
}

/// Lowers catch dispatch with finalizers and reports whether any catch reaches the post-try join.
pub(super) fn lower_catch_dispatch_with_finally(
    ctx: &mut LoweringContext<'_, '_>,
    catches: &[CatchClause],
    after_block: BlockId,
    finally_body: &[Stmt],
    span: Span,
) -> bool {
    let mut after_reachable = false;
    for catch in catches {
        let catch_body = ctx.builder.create_named_block("try.catch_body", Vec::new());
        let next_catch = ctx.builder.create_named_block("try.catch_next", Vec::new());
        lower_catch_match(ctx, catch, catch_body, next_catch, span);
        ctx.builder.position_at_end(catch_body);
        lower_catch_bind(ctx, catch, span);
        // The catch body runs under its OWN handler. An exception leaving it — a `throw`, or a
        // call that throws — must still run the `finally`, with that exception pending. Without
        // a handler here only an explicit `throw` reached the finalizer: a throwing CALL
        // skipped it entirely (MEASURED: `catch (X $c) { boom(); } finally { echo "F"; }` never
        // printed `F`; reference does, then propagates). Exits by jump pop it on the way out.
        let body_handler = ctx
            .builder
            .create_named_block("try.catch_body_unwind", Vec::new());
        let body_token = body_handler.as_raw() as i64;
        ctx.emit_void(
            Op::TryPushHandler,
            Vec::new(),
            Some(Immediate::I64(body_token)),
            Op::TryPushHandler.default_effects(),
            Some(span),
        );
        let depth = push_finally_frame(ctx, finally_body, false, Some((body_token, span)));
        ctx.push_try_loop_depth();
        lower_block(ctx, &catch.body);
        ctx.pop_try_loop_depth();
        pop_finally_frame_if_active(ctx, depth);
        if !ctx.builder.insertion_block_is_terminated() {
            emit_try_pop_handler(ctx, body_token, span);
            lower_block(ctx, finally_body);
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(ctx, after_block);
                after_reachable = true;
            }
        }
        ctx.clear_static_callable_locals();
        ctx.builder.position_at_end(body_handler);
        emit_try_pop_handler(ctx, body_token, span);
        lower_finally_with_pending_exception(ctx, finally_body, span);
        ctx.clear_static_callable_locals();
        ctx.builder.position_at_end(next_catch);
    }

    // No catch matched: the finalizer runs with the exception pending, then rethrows it.
    lower_finally_with_pending_exception(ctx, finally_body, span);
    after_reachable
}

/// Runs a `finally` body while an exception is pending, then rethrows that exception.
///
/// The pending exception is TAKEN into a hidden owned temp first, as
/// `catch (Throwable $t) { <finally>; throw $t; }` would, and each way out of the body settles
/// it the way php-src does (every case MEASURED on reference PHP 8.5.10):
///
/// - the body falls through: the exception is rethrown, its reference MOVED back to the
///   in-flight cell (load, forget the slot, throw — `throw $temp` handed the same reference on
///   while the slot kept it, and unwinding freed it under the in-flight exception);
/// - the body leaves by `return` or an included file's `return`: the exception is DISCARDED
///   at the jump (`control_exit::release_taken_exceptions`); peeking at it instead leaked one
///   object per call;
/// - an exception ESCAPES the body — a `throw`, or a call that throws: the pending exception is
///   appended to the END of its `previous` chain (`X:new <- X:orig`, and `new <- p <- orig` when
///   the new one already had a previous), then it propagates. elephc chained nothing and
///   leaked the pending exception. A `handler` around the body catches the escape, so a
///   throwing call is covered as well as a `throw`; an exception the body catches itself never
///   reaches it, and is not chained (`inner` stays alone), as in reference.
fn lower_finally_with_pending_exception(
    ctx: &mut LoweringContext<'_, '_>,
    finally_body: &[Stmt],
    span: Span,
) {
    let taken = bind_in_flight_exception(ctx, span);
    // Recorded BEFORE the chaining handler's frame is pushed, so a jump out of the body pops
    // that handler first and only then discards the exception.
    ctx.taken_finally_exceptions
        .push((taken.clone(), ctx.loop_stack.len(), ctx.finally_stack.len()));

    let chain_handler = ctx
        .builder
        .create_named_block("finally.chain_pending", Vec::new());
    let chain_token = chain_handler.as_raw() as i64;
    let fell_through = ctx.builder.create_named_block("finally.rethrow_pending", Vec::new());
    ctx.emit_void(
        Op::TryPushHandler,
        Vec::new(),
        Some(Immediate::I64(chain_token)),
        Op::TryPushHandler.default_effects(),
        Some(span),
    );
    // An empty frame: a jump out of the body only has to pop the handler.
    let depth = push_finally_frame(ctx, &[], false, Some((chain_token, span)));
    ctx.push_try_loop_depth();
    lower_block(ctx, finally_body);
    ctx.pop_try_loop_depth();
    pop_finally_frame_if_active(ctx, depth);
    let reaches_rethrow = !ctx.builder.insertion_block_is_terminated();
    if reaches_rethrow {
        emit_try_pop_handler(ctx, chain_token, span);
        branch_to(ctx, fell_through);
    }

    // An exception escaped the body: chain the pending one under it, then let it propagate.
    ctx.builder.position_at_end(chain_handler);
    ctx.clear_static_callable_locals();
    emit_try_pop_handler(ctx, chain_token, span);
    let escaped = ctx.emit_owned_value(
        Op::CatchBind,
        Vec::new(),
        None,
        PhpType::Object("Throwable".to_string()),
        Op::CatchBind.default_effects(),
        Some(span),
    );
    let pending = ctx.load_local(&taken, Some(span));
    ctx.clear_owned_hidden_temp(&taken, Some(span));
    ctx.emit_void(
        Op::RuntimeCall,
        vec![escaped.value, pending.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ExceptionChain)),
        crate::ir::Effects::all(),
        Some(span),
    );
    terminate_throw(ctx, escaped.value);

    ctx.taken_finally_exceptions.pop();
    ctx.builder.position_at_end(fell_through);
    if !reaches_rethrow {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    }
    ctx.clear_static_callable_locals();
    let value = ctx.load_local(&taken, Some(span));
    ctx.clear_owned_hidden_temp(&taken, Some(span));
    terminate_throw(ctx, value.value);
}

/// Takes and clears the in-flight exception into a hidden owned temporary, as a variable-less
/// `catch (Throwable)` would, and returns the temporary's name.
fn bind_in_flight_exception(ctx: &mut LoweringContext<'_, '_>, span: Span) -> String {
    let php_type = PhpType::Object("Throwable".to_string());
    let temp = ctx.declare_owned_hidden_temp(php_type.clone());
    let caught = ctx.emit_owned_value(
        Op::CatchBind,
        Vec::new(),
        None,
        php_type.clone(),
        Op::CatchBind.default_effects(),
        Some(span),
    );
    ctx.store_local(&temp, caught, php_type, Some(span));
    temp
}


/// Emits the match tests for one catch clause and branches to body or next clause.
pub(super) fn lower_catch_match(
    ctx: &mut LoweringContext<'_, '_>,
    catch: &CatchClause,
    catch_body: BlockId,
    next_catch: BlockId,
    span: Span,
) {
    if catch.exception_types.is_empty() {
        branch_to(ctx, next_catch);
        return;
    }

    for (idx, catch_type) in catch.exception_types.iter().enumerate() {
        let mismatch = if idx + 1 == catch.exception_types.len() {
            next_catch
        } else {
            ctx.builder
                .create_named_block("try.catch_type_next", Vec::new())
        };
        let current = lower_current_exception(ctx, span);
        let data = ctx.intern_class_name(catch_type.as_str());
        let matched = ctx.emit_value(
            Op::InstanceOf,
            vec![current.value],
            Some(Immediate::Data(data)),
            PhpType::Bool,
            Op::InstanceOf.default_effects(),
            Some(span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: matched.value,
            then_target: catch_body,
            then_args: Vec::new(),
            else_target: mismatch,
            else_args: Vec::new(),
        });
        if idx + 1 != catch.exception_types.len() {
            ctx.builder.position_at_end(mismatch);
        }
    }
}

/// Emits the current exception value as an object-typed SSA value.
pub(super) fn lower_current_exception(ctx: &mut LoweringContext<'_, '_>, span: Span) -> LoweredValue {
    ctx.emit_value(
        Op::CatchCurrent,
        Vec::new(),
        None,
        PhpType::Object("Throwable".to_string()),
        Op::CatchCurrent.default_effects(),
        Some(span),
    )
}

/// Takes and clears the active exception for a matched catch clause, then stores the
/// owned result through the ordinary variable-storage planner. This keeps local,
/// global, static, and reference-cell destinations consistent while preserving the
/// single in-flight reference transferred by the runtime (issue #448). A variable-less
/// catch consumes the reference through a hidden owned temporary so it follows the
/// same lifecycle instead of leaking.
pub(super) fn lower_catch_bind(ctx: &mut LoweringContext<'_, '_>, catch: &CatchClause, span: Span) {
    let php_type = catch_variable_type(catch);
    let variable = match catch.variable.as_ref() {
        Some(variable) => variable.clone(),
        None => ctx.declare_owned_hidden_temp(php_type.clone()),
    };
    let caught = ctx.emit_owned_value(
        Op::CatchBind,
        Vec::new(),
        None,
        php_type.clone(),
        Op::CatchBind.default_effects(),
        Some(span),
    );
    ctx.store_local(&variable, caught, php_type, Some(span));
}

/// Returns the local type to use for a catch variable.
pub(super) fn catch_variable_type(catch: &CatchClause) -> PhpType {
    if catch.exception_types.len() == 1 {
        return PhpType::Object(
            catch.exception_types[0]
                .trim_start_matches('\\')
                .to_string(),
        );
    }
    PhpType::Object("Throwable".to_string())
}
