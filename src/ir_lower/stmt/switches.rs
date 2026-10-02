//! Purpose:
//! Static and dynamic switch dispatch with PHP fallthrough.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;
use std::collections::HashMap;

use crate::ir_lower::context::IfArmExit;

/// Lowers a `switch` with source-ordered pattern evaluation and PHP fallthrough.
pub(super) fn lower_switch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) {
    let subject_span = subject.span;
    let subject = lower_expr(ctx, subject);
    let exit = ctx.builder.create_named_block("switch.exit", Vec::new());

    // The compact integer jump table is valid only for an integer scrutinee with
    // integer case labels. Any other subject (string, float, mixed) takes the
    // source-ordered dynamic path — see `lower_dynamic_switch_dispatch` for how it
    // picks PHP loose-equality vs the integer fast path per subject/case pair.
    let dispatch = if subject.ir_type == IrType::I64 && can_lower_static_switch(cases) {
        let subject = coerce_to_int(ctx, subject, None);
        lower_static_switch_dispatch(ctx, subject, cases)
    } else {
        lower_dynamic_switch_dispatch(ctx, subject, cases)
    };

    lower_switch_bodies(ctx, cases, default, dispatch, exit, subject_span);
}

/// The edges a switch dispatch leaves, each carrying the flow facts on that edge.
///
/// A case label is an arbitrary expression evaluated in source order, and it may assign a
/// local (`case ($x = null) === null:`), so the facts differ from one label to the next: a
/// case body starts from the facts of the labels that select it, never from those left after
/// the last label. `cases` holds one edge per label of each case, in source order; `default`
/// is the edge taken when no label matches.
pub(super) struct SwitchDispatch {
    cases: Vec<Vec<IfArmExit>>,
    default: IfArmExit,
}

/// Records the dispatch edge that reaches the unterminated block `tail`, with the current facts.
fn switch_dispatch_edge(ctx: &LoweringContext<'_, '_>, tail: BlockId) -> IfArmExit {
    IfArmExit {
        tail,
        types: ctx.local_types_snapshot(),
        initialized: ctx.initialized_slots_snapshot(),
        static_callables: HashMap::new(),
    }
}

/// Returns true when every switch case pattern can use the static integer switch terminator.
pub(super) fn can_lower_static_switch(cases: &[(Vec<Expr>, Vec<Stmt>)]) -> bool {
    cases
        .iter()
        .flat_map(|(case_exprs, _)| case_exprs)
        .all(|case_expr| int_case_value(case_expr).is_some())
}

/// Emits the compact integer-switch dispatch for statically-known case values.
///
/// Integer labels have no side effects, so every edge carries the facts at the dispatch.
pub(super) fn lower_static_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: LoweredValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
) -> SwitchDispatch {
    let blocks = cases
        .iter()
        .map(|_| ctx.builder.create_named_block("switch.case", Vec::new()))
        .collect::<Vec<_>>();
    let default_block = ctx.builder.create_named_block("switch.default", Vec::new());
    let mut switch_cases = Vec::new();
    for ((case_exprs, _), case_block) in cases.iter().zip(&blocks) {
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
    SwitchDispatch {
        cases: blocks
            .iter()
            .map(|block| vec![switch_dispatch_edge(ctx, *block)])
            .collect(),
        default: switch_dispatch_edge(ctx, default_block),
    }
}

/// Emits source-ordered dynamic switch pattern checks for non-literal case expressions.
///
/// PHP `switch` compares the subject against each case with loose equality (`==`).
/// String subjects/labels and float/numeric pairs are dispatched through `Op::LooseEq`
/// so the comparison honors PHP string/numeric coercion rules (`switch (1.5)` matching
/// `case 1.5`, not `case 1`); purely integer-like subject-and-case pairs keep the
/// cheaper `coerce_to_int` + `ICmp` fast path.
///
/// Each label that matches branches to its own `switch.match` block, whose edge records the
/// facts as of that label: a label's expression may assign a local that a later label changes.
pub(super) fn lower_dynamic_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: LoweredValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
) -> SwitchDispatch {
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
    let mut case_edges = Vec::with_capacity(cases.len());
    for (case_exprs, _) in cases {
        let mut edges = Vec::with_capacity(case_exprs.len());
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
            let matched_block = ctx.builder.create_named_block("switch.match", Vec::new());
            let miss_block = ctx.builder.create_named_block("switch.next", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: matched.value,
                then_target: matched_block,
                then_args: Vec::new(),
                else_target: miss_block,
                else_args: Vec::new(),
            });
            edges.push(switch_dispatch_edge(ctx, matched_block));
            ctx.builder.position_at_end(miss_block);
        }
        case_edges.push(edges);
    }
    let default_block = ctx.builder.create_named_block("switch.default", Vec::new());
    branch_to(ctx, default_block);
    let default = switch_dispatch_edge(ctx, default_block);
    ctx.clear_static_callable_locals();
    SwitchDispatch {
        cases: case_edges,
        default,
    }
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
///
/// Each body is entered from the dispatch and, unless the body before it ended in `break`,
/// by falling through; the exit is reached by every `break` and by the last body falling out.
/// Those are joins exactly like an `if` merge, and lowering the bodies one after another used
/// to leave each join with the facts of whichever body was lowered last: after
/// `case 1: $o = null; break; case 2: …; default: …` the exit read `$o` as `null` on the path
/// that kept the object. Every body therefore starts from its own dispatch edges (see
/// [`SwitchDispatch`]) joined with its fall-through edge, and the exit joins every edge that reaches it
/// (`finish_if_type_join`), boxing a local whose edges disagree on its representation.
pub(super) fn lower_switch_bodies(
    ctx: &mut LoweringContext<'_, '_>,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    dispatch: SwitchDispatch,
    exit: BlockId,
    span: Span,
) {
    let default_index = default
        .and_then(|default| switch_default_source_index(cases, default))
        .unwrap_or(cases.len());
    ctx.clear_static_callable_locals();
    let mut case_edges = dispatch.cases.into_iter();
    let mut default_edge = Some(dispatch.default);
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
    ctx.switch_exit_arms.push((exit, Vec::new()));
    let mut fallthrough = None;
    for index in 0..=cases.len() {
        if default.is_some() && default_index == index {
            let edge = default_edge.take().expect("the default edge is entered once");
            enter_switch_body(ctx, vec![edge], fallthrough.take(), span);
            if let Some(default) = default {
                lower_block(ctx, default);
            }
            fallthrough = leave_switch_body(ctx);
        }
        if let Some((_, body)) = cases.get(index) {
            let edges = case_edges.next().expect("the dispatch left edges for every case");
            enter_switch_body(ctx, edges, fallthrough.take(), span);
            lower_block(ctx, body);
            fallthrough = leave_switch_body(ctx);
        }
    }
    let (_, mut exit_edges) = ctx
        .switch_exit_arms
        .pop()
        .expect("switch exit join pushed above");
    exit_edges.extend(fallthrough);
    if default.is_none() {
        exit_edges.extend(default_edge.take());
    }
    ctx.loop_stack.pop();
    join_switch_edges(ctx, exit_edges, exit, span);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Positions the builder at the start of one switch body, joining its incoming edges.
///
/// `dispatch` holds the edges from the dispatch that select this body, one per matching label.
/// A body reached by exactly one edge starts in that edge's block with its facts; otherwise its
/// dispatch edges and its fall-through edge are joined into a fresh block like the arms of an
/// `if`.
fn enter_switch_body(
    ctx: &mut LoweringContext<'_, '_>,
    mut dispatch: Vec<IfArmExit>,
    fallthrough: Option<IfArmExit>,
    span: Span,
) {
    dispatch.extend(fallthrough);
    if dispatch.len() == 1 {
        let edge = dispatch.pop().expect("one edge");
        ctx.restore_local_types(edge.types);
        ctx.restore_initialized_slots(edge.initialized);
        ctx.builder.position_at_end(edge.tail);
        return;
    }
    let body = ctx.builder.create_named_block("switch.body", Vec::new());
    join_switch_edges(ctx, dispatch, body, span);
    ctx.builder.position_at_end(body);
}

/// Joins switch edges into `merge` like the arms of an `if`, starting from the first edge's facts.
///
/// `finish_if_type_join` rewrites only the locals its arms disagree on and leaves every other
/// local as the context holds it. After an `if` that is the last arm's facts, but a switch body
/// is entered after the WHOLE dispatch was lowered, so the context holds the facts of the last
/// label: a local the last label assigned (`case ($o = null) === null:`) would read as that
/// label left it in an earlier body whose own edges all agree it still holds the object.
/// Initialization is a must-fact: only slots initialized on every incoming edge are visible
/// in the body or exit. Restore that intersection after any type-join conversion stores.
fn join_switch_edges(
    ctx: &mut LoweringContext<'_, '_>,
    edges: Vec<IfArmExit>,
    merge: BlockId,
    span: Span,
) {
    let mut initialized = HashSet::new();
    if let Some(first) = edges.first() {
        ctx.restore_local_types(first.types.clone());
        ctx.restore_initialized_slots(first.initialized.clone());
        initialized = first.initialized.clone();
        for edge in edges.iter().skip(1) {
            initialized.retain(|slot| edge.initialized.contains(slot));
        }
    }
    finish_if_type_join(ctx, edges, merge, span);
    ctx.restore_initialized_slots(initialized);
}

/// Ends one switch body, deferring its fall-through edge when control can still leave it.
fn leave_switch_body(ctx: &mut LoweringContext<'_, '_>) -> Option<IfArmExit> {
    let fallthrough = if ctx.builder.insertion_block_is_terminated() {
        None
    } else {
        let mut edges = Vec::with_capacity(1);
        record_if_arm_exit(ctx, &mut edges);
        edges.pop()
    };
    ctx.clear_static_callable_locals();
    fallthrough
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

/// Returns true when `span` appears before `pivot` in the same source file.
pub(super) fn span_is_before(span: Span, pivot: Span) -> bool {
    span.line < pivot.line || (span.line == pivot.line && span.col < pivot.col)
}
