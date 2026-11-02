//! Purpose:
//! Include and include-once statement lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a resolver-preserved runtime include/require through the Magician bridge.
pub(super) fn lower_include(
    ctx: &mut LoweringContext<'_, '_>,
    path: &Expr,
    once: bool,
    required: bool,
    span: Span,
) {
    let path = lower_expr(ctx, path);
    let call = ctx.emit_value(
        Op::RuntimeCall,
        vec![path.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::DynamicInclude {
            once,
            required,
            strict_php: crate::strict_php::is_enabled(),
            // An include written as a statement: `release_expr_statement_result` below throws
            // the value away, which is what lets the runtime skip a compiler-included source.
            value_discarded: true,
        })),
        PhpType::Mixed,
        effects_lookup::runtime_effects(),
        Some(span),
    );
    release_expr_statement_result(ctx, call, span);
    // A closed-world include runs a compiled unit in its own scope (or none at all): the caller's
    // locals cannot change, so there is no barrier and no interpreter context.
    if !crate::closed_world::is_enabled() {
        ctx.mark_eval_executed();
        ctx.apply_eval_barrier();
    }
    ctx.invalidate_static_callable_locals();
}

/// Lowers an include-once marker.
pub(super) fn lower_include_once_mark(ctx: &mut LoweringContext<'_, '_>, source_path: &std::path::Path, span: Span) {
    let source = source_immediate(ctx, source_path);
    ctx.emit_void(
        Op::IncludeOnceMark,
        Vec::new(),
        Some(source),
        Op::IncludeOnceMark.default_effects(),
        Some(span),
    );
}

/// Lowers an include-once guarded body.
pub(super) fn lower_include_once_guard(
    ctx: &mut LoweringContext<'_, '_>,
    source_path: &std::path::Path,
    required: bool,
    body: &[Stmt],
    span: Span,
) {
    let source = source_immediate(ctx, source_path);
    let source_id = match &source {
        Immediate::Source(id) => Some(*id),
        _ => None,
    };
    let should_run = ctx
        .builder
        .emit_with_effects(
            Op::IncludeOnceGuard,
            Vec::new(),
            Some(source),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::IncludeOnceGuard.default_effects(),
            Some(span),
        )
        .expect("include_once_guard produces a branch condition");
    let body_block = ctx
        .builder
        .create_named_block("include_once_body", Vec::new());
    let after_block = ctx
        .builder
        .create_named_block("include_once_after", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: should_run,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: after_block,
        else_args: Vec::new(),
    });
    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    // A resolved include body is inlined here, so PHP's own `require_once('<path>')` frame would
    // otherwise disappear from every trace. Republish it around the body on the activation chain
    // the backtrace and Throwable walkers already read; the exit runs only when the body falls
    // through, and an exception unwinds the record itself.
    let frame = source_id.map(|source| {
        let slot = ctx.include_frame_slots;
        ctx.include_frame_slots = ctx.include_frame_slots.saturating_add(1);
        Immediate::IncludeFrame(Box::new(crate::ir::IncludeFrameImmediate {
            source,
            required,
            slot,
        }))
    });
    if let Some(frame) = &frame {
        ctx.emit_void(
            Op::IncludeFrameEnter,
            Vec::new(),
            Some(frame.clone()),
            Op::IncludeFrameEnter.default_effects(),
            Some(span),
        );
    }
    let surrounding_try_handler_stack = ctx.try_handler_stack.clone();
    lower_block(ctx, body);
    if let Some(frame) = frame {
        if !ctx.builder.insertion_block_is_terminated() {
            ctx.emit_void(
                Op::IncludeFrameExit,
                Vec::new(),
                Some(frame),
                Op::IncludeFrameExit.default_effects(),
                Some(span),
            );
        }
    }
    branch_to(ctx, after_block);
    ctx.builder.position_at_end(after_block);
    ctx.try_handler_stack = surrounding_try_handler_stack;
    ctx.clear_static_callable_locals();
}

/// Resolves against frozen input metadata; a missing input is a compile diagnostic.
fn source_immediate(ctx: &LoweringContext<'_, '_>, path: &std::path::Path) -> Immediate {
    match ctx.source_catalog.as_ref().and_then(|catalog| catalog.id_for_path(path)) {
        Some(id) => Immediate::Source(id),
        None => Immediate::UnresolvedSource(Box::new(path.to_path_buf())),
    }
}
