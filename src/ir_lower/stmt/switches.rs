//! Purpose:
//! Static and dynamic switch dispatch with PHP fallthrough.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a `switch` with source-ordered pattern evaluation and PHP fallthrough.
pub(super) fn lower_switch(
    ctx: &mut LoweringContext<'_, '_>,
    subject_expr: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) {
    let span = subject_expr.span;
    let live_subject = if let ExprKind::Variable(name) = &subject_expr.kind {
        Some(name.as_str())
    } else { None };
    let static_variable = live_subject.is_some_and(|name| {
        ctx.local_load_php_type(name).is_some_and(|ty| ty.codegen_repr() == PhpType::Int)
            && can_lower_static_switch(cases)
    });
    let (subject, owner) = if live_subject.is_some() && !static_variable {
        // Dynamic variable subjects are read only at their comparison points. An
        // unused initial read can itself allocate an owning local conversion.
        (None, None)
    } else {
        let mut subject = lower_expr(ctx, subject_expr);
        let preserve_subject = live_subject.is_none()
            && matches!(subject.ir_type, IrType::Heap(crate::ir::IrHeapKind::Object | crate::ir::IrHeapKind::Mixed))
            && ctx.classes.keys().any(|name| elephc_builtin_contract::lookup_class(name)
                .is_some_and(|class| class.operators.is_some()));
        if preserve_subject {
            if matches!(subject.ir_type, IrType::Heap(crate::ir::IrHeapKind::Mixed)) {
                subject = crate::ir_lower::ownership::copy_assignment_value(ctx, subject, Some(span));
            } else {
                let captured = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, subject, Some(span));
                if ctx.value_is_owning_temporary(subject) {
                    crate::ir_lower::ownership::release_if_owned(ctx, subject, Some(span));
                }
                subject = captured;
            }
        }
        let (subject, owner) = if preserve_subject
            || ctx.builder.value_defining_op(subject.value) == Some(Op::StrCharAt)
        {
            crate::ir_lower::expr::root_owned_call_operand(ctx, subject, span)
        } else { (subject, None) };
        (Some(subject), owner)
    };
    let exit = ctx.builder.create_named_block("switch.exit", Vec::new());
    let default_block = ctx.builder.create_named_block("switch.default", Vec::new());
    let blocks = cases
        .iter()
        .map(|_| ctx.builder.create_named_block("switch.case", Vec::new()))
        .collect::<Vec<_>>();
    let dispatch_blocks = if owner.is_some() {
        blocks.iter().map(|_| ctx.builder.create_named_block("switch.matched", Vec::new())).collect()
    } else { blocks.clone() };
    let dispatch_default = if owner.is_some() {
        ctx.builder.create_named_block("switch.unmatched", Vec::new())
    } else { default_block };

    // The compact integer jump table is valid only for an integer scrutinee with
    // integer case labels. Any other subject (string, float, mixed) takes the
    // source-ordered dynamic path. See `lower_dynamic_switch_dispatch` for how it
    // picks PHP loose-equality vs the integer fast path per subject/case pair.
    if let Some(subject) = subject.filter(|subject| subject.ir_type == IrType::I64
        && ctx.builder.value_php_type(subject.value).codegen_repr() == PhpType::Int
        && can_lower_static_switch(cases))
    {
        let subject = coerce_to_int(ctx, subject, None);
        lower_static_switch_dispatch(ctx, subject, cases, &dispatch_blocks, dispatch_default);
    } else {
        lower_dynamic_switch_dispatch(ctx, subject, live_subject, cases, &dispatch_blocks, dispatch_default);
    }
    if let Some(owner) = owner {
        for (dispatch, target) in dispatch_blocks.iter().zip(&blocks)
            .chain(std::iter::once((&dispatch_default, &default_block)))
        {
            ctx.builder.position_at_end(*dispatch);
            crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
            branch_to(ctx, *target);
        }
    }

    lower_switch_bodies(ctx, cases, default, &blocks, default_block, exit);
}

/// Returns true when every switch case pattern can use the static integer switch terminator.
pub(super) fn can_lower_static_switch(cases: &[(Vec<Expr>, Vec<Stmt>)]) -> bool {
    cases
        .iter()
        .flat_map(|(case_exprs, _)| case_exprs)
        .all(|case_expr| int_case_value(case_expr).is_some())
}

/// Emits the compact integer-switch dispatch for statically-known case values.
pub(super) fn lower_static_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: LoweredValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    blocks: &[BlockId],
    default_block: BlockId,
) {
    let mut switch_cases = Vec::new();
    for ((case_exprs, _), case_block) in cases.iter().zip(blocks) {
        for case_expr in case_exprs {
            let Some(value) = int_case_value(case_expr) else {
                continue;
            };
            switch_cases.push(SwitchCase {
                value,
                target: *case_block,
                args: Vec::new(),
            });
        }
    }
    ctx.builder.terminate(Terminator::Switch {
        scrutinee: subject.value,
        cases: switch_cases,
        default: default_block,
        default_args: Vec::new(),
    });
    ctx.clear_static_callable_locals();
}

/// Emits source-ordered dynamic switch pattern checks for non-literal case expressions.
///
/// PHP `switch` compares the subject against each case with loose equality (`==`).
/// Only pairs of PHP integers use `ICmp`. All other pairs use `Op::LooseEq` so
/// bool, null, array, object and numeric-string comparisons retain their PHP rules.
pub(super) fn lower_dynamic_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: Option<LoweredValue>,
    live_subject: Option<&str>,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    blocks: &[BlockId],
    default_block: BlockId,
) {
    for ((case_exprs, _), case_block) in cases.iter().zip(blocks) {
        for case_expr in case_exprs {
            let case_value = lower_expr(ctx, case_expr);
            // PHP keeps a simple variable subject live across label evaluation and
            // comparison callbacks. Reload its storage without reevaluating expressions.
            let subject = match live_subject {
                Some(name) => ctx.load_local(name, None),
                None => subject.expect("non-variable switch subject is captured once"),
            };
            let subject = ctx.retype_packed_address_for_pointer_sink(subject, &PhpType::Pointer(None), None);
            let subject_php = ctx.builder.value_php_type(subject.value).codegen_repr();
            let subject_is_pointer = matches!(subject_php, PhpType::Pointer(_));
            let int_subject = (subject.ir_type == IrType::I64 && subject_php == PhpType::Int)
                .then(|| coerce_to_int(ctx, subject, None));
            // A PHP null case keeps its `Void` type unless the subject is a pointer (where the
            // null sentinel must stay a pointer word). Retyping it into `Pointer(None)` hid it
            // from the mixed/string loose-equality paths and made the backend refuse the whole
            // switch (#1994). Packed cases still need the pointer sink below.
            let case_is_null = !subject_is_pointer
                && matches!(
                    ctx.builder.value_php_type(case_value.value).codegen_repr(),
                    PhpType::Void
                );
            let case_value = if case_is_null {
                // `int == null` is `int == 0`; materialize that cast only for
                // the integer shortcut, preserving null for all other PHP types.
                if int_subject.is_some() {
                    lower_expr(ctx, &Expr::new(ExprKind::IntLiteral(0), case_expr.span))
                } else {
                    case_value
                }
            } else {
                ctx.retype_packed_address_for_pointer_sink(
                    case_value,
                    &PhpType::Pointer(None),
                    None,
                )
            };
            // Two pointers (packed record addresses are retyped above) match by address identity;
            // the integer `icmp` path does not accept pointer words. A pointer against a
            // non-pointer takes the loose-equality path below, which keeps its diagnostic.
            let case_is_pointer =
                matches!(ctx.builder.value_php_type(case_value.value).codegen_repr(), PhpType::Pointer(_));
            if subject_is_pointer && case_is_pointer {
                let matched = ctx.emit_value(
                    Op::StrictEq,
                    vec![subject.value, case_value.value],
                    None,
                    PhpType::Bool,
                    Op::StrictEq.default_effects(),
                    Some(case_expr.span),
                );
                let miss_block = ctx.builder.create_named_block("switch.next", Vec::new());
                ctx.builder.terminate(Terminator::CondBr {
                    cond: matched.value,
                    then_target: *case_block,
                    then_args: Vec::new(),
                    else_target: miss_block,
                    else_args: Vec::new(),
                });
                ctx.builder.position_at_end(miss_block);
                continue;
            }
            // Storage width does not establish PHP comparison semantics. Both operands
            // must be PHP integers before ICmp can replace loose equality.
            let use_loose_eq = int_subject.is_none()
                || case_value.ir_type != IrType::I64
                || ctx.builder.value_php_type(case_value.value).codegen_repr() != PhpType::Int;
            // Labels are evaluated before a live subject reload. Both can retain
            // an independent payload when a later store widens their frame slots.
            let local_guards = if live_subject.is_some() {
                ctx.guard_owned_local_views(&[case_value, subject], case_expr.span)
            } else {
                ctx.guard_owned_local_views(&[case_value], case_expr.span)
            };
            let matched = if use_loose_eq {
                // Loose equality handles string/string, string/scalar, float/numeric,
                // and mixed cases exactly as PHP's `==` would inside an if/elseif chain.
                let expression = Expr::new(ExprKind::Null, case_expr.span);
                let compare = |ctx: &mut LoweringContext<'_, '_>, pin_operands| {
                    let pins = if pin_operands {
                        crate::ir_lower::expr::pin_in_flight_owners(
                            ctx, &[subject.value, case_value.value], case_expr.span,
                        )
                    } else { Vec::new() };
                    let result = ctx.emit_value(
                        Op::LooseEq, vec![subject.value, case_value.value], None,
                        PhpType::Bool, Op::LooseEq.default_effects(), Some(case_expr.span),
                    );
                    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, case_expr.span);
                    result
                };
                crate::ir_lower::expr::object_operators::lower_dynamic_object_binary(
                    ctx, subject, &crate::parser::ast::BinOp::Eq, case_value,
                    &expression, false, |ctx| compare(ctx, false),
                ).unwrap_or_else(|| compare(ctx, true))
            } else {
                let case_value = coerce_to_int(ctx, case_value, Some(case_expr.span));
                ctx.emit_value(
                    Op::ICmp,
                    vec![
                        int_subject
                            .expect("integer subject is pre-coerced")
                            .value,
                        case_value.value,
                    ],
                    Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
                    PhpType::Bool,
                    Op::ICmp.default_effects(),
                    Some(case_expr.span),
                )
            };
            if local_guards.contains(&case_value.value) {
                ctx.unguard_call_argument(case_value.value, case_expr.span);
            }
            if use_loose_eq && ctx.value_needs_release_after_use(case_value) {
                crate::ir_lower::ownership::release_if_owned(ctx, case_value, Some(case_expr.span));
            }
            if local_guards.contains(&subject.value) {
                ctx.unguard_call_argument(subject.value, case_expr.span);
            }
            if live_subject.is_some() && ctx.value_needs_release_after_use(subject) {
                // Finalization prunes this release when the local lends its storage,
                // and keeps it when widening makes the reload an owning conversion.
                crate::ir_lower::ownership::release_if_owned(ctx, subject, Some(case_expr.span));
            }
            let miss_block = ctx.builder.create_named_block("switch.next", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: matched.value,
                then_target: *case_block,
                then_args: Vec::new(),
                else_target: miss_block,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(miss_block);
        }
    }
    branch_to(ctx, default_block);
    ctx.clear_static_callable_locals();
}

/// Lowers switch case/default bodies and preserves PHP fallthrough between adjacent bodies.
pub(super) fn lower_switch_bodies(
    ctx: &mut LoweringContext<'_, '_>,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    blocks: &[BlockId],
    default_block: BlockId,
    exit: BlockId,
) {
    let default_index = default
        .and_then(|default| switch_default_source_index(cases, default))
        .unwrap_or(cases.len());
    ctx.clear_static_callable_locals();
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: exit,
        cleanup: None,
        source_owner: None,
        source_pin: None,
        iterator_owner: None,
        iterator_cleanup: None,
        receiver_pin: None,
    });
    for index in 0..=cases.len() {
        if default.is_some() && default_index == index {
            ctx.builder.position_at_end(default_block);
            if let Some(default) = default {
                lower_block(ctx, default);
            }
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(ctx, blocks.get(index).copied().unwrap_or(exit));
            }
            ctx.clear_static_callable_locals();
        }
        if let Some((_, body)) = cases.get(index) {
            ctx.builder.position_at_end(blocks[index]);
            lower_block(ctx, body);
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(
                    ctx,
                    switch_next_body_block(index + 1, blocks, default_index, default_block, exit),
                );
            }
            ctx.clear_static_callable_locals();
        }
    }
    if default.is_none() {
        ctx.builder.position_at_end(default_block);
        branch_to(ctx, exit);
    }
    ctx.loop_stack.pop();
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Returns the source-order insertion point for a non-empty switch default body.
pub(super) fn switch_default_source_index(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &[Stmt],
) -> Option<usize> {
    if cases.is_empty() {
        return Some(0);
    }
    let default_start = default.first()?.span;
    if default_start == Span::dummy() {
        return None;
    }
    let mut default_index = 0;
    for (conditions, _) in cases {
        let case_start = conditions.first()?.span;
        if case_start == Span::dummy() {
            return None;
        }
        if span_is_before(case_start, default_start) {
            default_index += 1;
        }
    }
    Some(default_index)
}

/// Returns the block that follows one source-ordered switch body.
pub(super) fn switch_next_body_block(
    next_index: usize,
    blocks: &[BlockId],
    default_index: usize,
    default_block: BlockId,
    exit: BlockId,
) -> BlockId {
    if default_index == next_index {
        default_block
    } else {
        blocks.get(next_index).copied().unwrap_or(exit)
    }
}

/// Returns true when `span` appears before `pivot` in the same source file.
pub(super) fn span_is_before(span: Span, pivot: Span) -> bool {
    span.line < pivot.line || (span.line == pivot.line && span.col < pivot.col)
}
