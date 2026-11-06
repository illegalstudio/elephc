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

    // The compact integer jump table is valid only for an integer scrutinee with
    // integer case labels. Any other subject (string, float, mixed) takes the
    // source-ordered dynamic path. See `lower_dynamic_switch_dispatch` for how it
    // picks PHP loose-equality vs the integer fast path per subject/case pair.
    let dispatch = if let Some(subject) = subject.filter(|subject| subject.ir_type == IrType::I64
        && ctx.builder.value_php_type(subject.value).codegen_repr() == PhpType::Int
        && can_lower_static_switch(cases))
    {
        let subject = coerce_to_int(ctx, subject, None);
        lower_static_switch_dispatch(ctx, subject, cases)
    } else {
        lower_dynamic_switch_dispatch(ctx, subject, live_subject, cases)
    };
    if let Some(owner) = owner {
        // Every dispatch edge ends in its own unterminated block, so the rooted subject is
        // retired there before the edge enters (or is joined into) its body.
        for edge in dispatch.cases.iter().flatten().chain(std::iter::once(&dispatch.default)) {
            ctx.builder.position_at_end(edge.tail);
            crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
        }
    }

    lower_switch_bodies(ctx, cases, default, dispatch, exit, span);
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
        reflection: ctx.reflection_bindings_snapshot(),
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
/// Only pairs of PHP integers use `ICmp`. All other pairs use `Op::LooseEq` so
/// bool, null, array, object and numeric-string comparisons retain their PHP rules.
///
/// Each label that matches branches to its own `switch.matched` block, whose edge records the
/// facts as of that label: a label's expression may assign a local that a later label changes.
pub(super) fn lower_dynamic_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: Option<LoweredValue>,
    live_subject: Option<&str>,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
) -> SwitchDispatch {
    let mut case_edges = Vec::with_capacity(cases.len());
    for (case_exprs, _) in cases {
        let mut edges = Vec::with_capacity(case_exprs.len());
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
                let matched_block = ctx.builder.create_named_block("switch.matched", Vec::new());
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
            let matched_block = ctx.builder.create_named_block("switch.matched", Vec::new());
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
    ctx.join_edge_arms.push((exit, Vec::new()));
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
    let mut exit_edges = take_join_edges(ctx, exit);
    exit_edges.extend(fallthrough);
    if default.is_none() {
        exit_edges.extend(default_edge.take());
    }
    ctx.loop_stack.pop();
    join_edges(ctx, exit_edges, exit, span, ContainerJoin::Reconcile);
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
    join_edges(ctx, dispatch, body, span, ContainerJoin::Reconcile);
    ctx.builder.position_at_end(body);
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
