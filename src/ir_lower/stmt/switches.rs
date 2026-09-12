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
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) {
    let subject = lower_expr(ctx, subject);
    let exit = ctx.builder.create_named_block("switch.exit", Vec::new());
    let default_block = ctx.builder.create_named_block("switch.default", Vec::new());
    let blocks = cases
        .iter()
        .map(|_| ctx.builder.create_named_block("switch.case", Vec::new()))
        .collect::<Vec<_>>();

    // The compact integer jump table is valid only for an integer scrutinee with
    // integer case labels. Any other subject (string, float, mixed) takes the
    // source-ordered dynamic path — see `lower_dynamic_switch_dispatch` for how it
    // picks PHP loose-equality vs the integer fast path per subject/case pair.
    if subject.ir_type == IrType::I64 && can_lower_static_switch(cases) {
        let subject = coerce_to_int(ctx, subject, None);
        lower_static_switch_dispatch(ctx, subject, cases, &blocks, default_block);
    } else {
        lower_dynamic_switch_dispatch(ctx, subject, cases, &blocks, default_block);
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
/// String subjects/labels and float/numeric pairs are dispatched through `Op::LooseEq`
/// so the comparison honors PHP string/numeric coercion rules (`switch (1.5)` matching
/// `case 1.5`, not `case 1`); purely integer-like subject-and-case pairs keep the
/// cheaper `coerce_to_int` + `ICmp` fast path.
pub(super) fn lower_dynamic_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: LoweredValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    blocks: &[BlockId],
    default_block: BlockId,
) {
    let subject_is_str = subject.ir_type == IrType::Str;
    let subject_is_mixed = matches!(subject.ir_type, IrType::Heap(crate::ir::IrHeapKind::Mixed));
    // Non-string, non-Mixed subjects are coerced to an integer once and reused by the ICmp path.
    // Mixed subjects must use loose equality for every case because the runtime tag may be
    // float, string, bool, etc. — coercing to int would truncate a float (issue #397).
    let int_subject = if subject_is_str || subject_is_mixed {
        None
    } else {
        Some(coerce_to_int(ctx, subject, None))
    };
    for ((case_exprs, _), case_block) in cases.iter().zip(blocks) {
        for case_expr in case_exprs {
            let case_value = lower_expr(ctx, case_expr);
            // Strings and floats must use loose equality: coercing a string to int
            // collapses every case to `0 == 0`, and coercing a float to int would
            // truncate the subject (so `switch (1.5) { case 1.5; }` would wrongly
            // match `case 1`). The cheap ICmp fast path stays for integer-like pairs.
            // Mixed subjects must always use loose equality (tag-aware comparison).
            let use_loose_eq = subject_is_str
                || subject_is_mixed
                || case_value.ir_type == IrType::Str
                || float_loose_eq_pair(subject.ir_type, case_value.ir_type);
            let matched = if use_loose_eq {
                // Loose equality handles string/string, string/scalar, float/numeric,
                // and mixed cases exactly as PHP's `==` would inside an if/elseif chain.
                ctx.emit_value(
                    Op::LooseEq,
                    vec![subject.value, case_value.value],
                    None,
                    PhpType::Bool,
                    Op::LooseEq.default_effects(),
                    Some(case_expr.span),
                )
            } else {
                let case_value = coerce_to_int(ctx, case_value, Some(case_expr.span));
                ctx.emit_value(
                    Op::ICmp,
                    vec![
                        int_subject
                            .expect("non-string subject is pre-coerced")
                            .value,
                        case_value.value,
                    ],
                    Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
                    PhpType::Bool,
                    Op::ICmp.default_effects(),
                    Some(case_expr.span),
                )
            };
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

/// Returns true when a switch subject/case pair must compare via float loose equality:
/// at least one side is a statically-typed float and both are numeric (`int`/`float`).
/// These pairs route through `Op::LooseEq`, which promotes both operands to float, so the
/// subject is not truncated to int (the backend supports float-vs-int loose equality).
///
/// An untyped (`Mixed`) subject holding a float is not covered here: it still takes the
/// integer fast path and truncates, a separate pre-existing loose-equality limitation that
/// needs a tag-aware runtime comparison helper (tracked in issue #397).
pub(super) fn float_loose_eq_pair(subject_ty: IrType, case_ty: IrType) -> bool {
    let numeric = |ty: IrType| matches!(ty, IrType::I64 | IrType::F64);
    (subject_ty == IrType::F64 || case_ty == IrType::F64) && numeric(subject_ty) && numeric(case_ty)
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
