//! Purpose:
//! Lowers boolean conditions directly to their true and false control-flow edges.
//!
//! Called from:
//! - Statement conditionals and loops, and full ternary expression lowering.
//!
//! Key details:
//! - Logical operators preserve lazy source-order evaluation without boolean merge slots.
//! - Whole logical trees allocate operand blocks in source order for fallthrough edges.
//! - Leaf operands use ordinary expression lowering and consuming PHP truthiness.

use super::*;

/// Branches on a condition without materializing intermediate logical results.
///
/// Expression nesting matches `lower_expr`, including for reference-returning operands.
/// Value-producing consumers keep using `lower_expr` instead of this branch-only entry point.
/// Maximum full prefix paths recorded for one logical condition.
const MAX_LOGICAL_PATHS: usize = 128;
/// Maximum leaves whose 2-bit fields fit one 64-bit path-state word.
const MAX_PATH_LEAVES: usize = 31;

pub(crate) fn lower_condition(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_target: BlockId,
    else_target: BlockId,
) {
    let span = condition.span;
    let logical = crate::ir_lower::debug_build_enabled()
        && span.source_path().is_some()
        && span.line > 0
        && condition_has_logical_operator(condition);
    let plan = logical.then(|| plan_full_paths(condition));
    let state = plan.as_ref().and_then(|plan| {
        if plan.paths.is_empty() || plan.leaf_spans.len() > MAX_PATH_LEAVES {
            return None;
        }
        let name = ctx.declare_hidden_temp(PhpType::Int);
        let slot = ctx.local_slots.get(&name).copied()?;
        let zero = ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(0)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(span),
        );
        ctx.store_local(&name, zero, PhpType::Int, Some(span));
        Some((name, slot))
    });
    let mut continuations = Vec::new();
    plan_condition_blocks(ctx, condition, &mut continuations);
    let mut continuations = continuations
        .into_iter()
        .map(|block| block.expect("every logical node has a continuation block"));
    let mut then_blocks = Vec::new();
    let mut else_blocks = Vec::new();
    let mut leaf_index = 0;
    lower_condition_tree(
        ctx,
        condition,
        then_target,
        else_target,
        &mut continuations,
        state.as_ref().map(|(name, _)| name.as_str()),
        &mut leaf_index,
        &mut then_blocks,
        &mut else_blocks,
    );
    debug_assert!(continuations.next().is_none());
    if let (Some(plan), Some((_, slot))) = (plan, state) {
        if !plan.paths.is_empty() && leaf_index == plan.leaf_spans.len() {
            let descriptors = plan
                .paths
                .iter()
                .map(|path| crate::ir::PathDescriptor {
                    mask: path_mask(&path.conditions),
                    label: path_label(path, &plan.leaf_spans),
                })
                .collect();
            let mut terminals: Vec<crate::ir::PathTerminal> = Vec::new();
            for (index, path) in plan.paths.iter().enumerate() {
                let block = if path.terminal_outcome {
                    then_blocks.get(path.terminal_leaf)
                } else {
                    else_blocks.get(path.terminal_leaf)
                };
                if let Some(block) = block {
                    if let Some(terminal) = terminals.iter_mut().find(|terminal| terminal.block == *block) {
                        terminal.paths.push(index);
                    } else {
                        terminals.push(crate::ir::PathTerminal {
                            block: *block,
                            paths: vec![index],
                        });
                    }
                }
            }
            if !terminals.is_empty() {
                ctx.builder.record_path_site(crate::ir::PathSite {
                    span,
                    kind: crate::ir::PathSiteKind::Logical,
                    state_local: Some(slot),
                    switch_branch_site: None,
                    paths: descriptors,
                    terminals,
                    truncated: plan.truncated,
                });
            }
        }
    }
}

/// Full-path planning result for one logical condition.
struct LogicalPathPlan {
    /// Leaf spans in source order.
    leaf_spans: Vec<Span>,
    /// Every full prefix route, up to the bounded cap.
    paths: Vec<FullPath>,
    /// Whether the path or leaf cap truncated the plan.
    truncated: bool,
}

/// One full logical prefix route.
struct FullPath {
    /// Evaluated leaf outcomes in source order.
    conditions: Vec<(usize, bool)>,
    /// Truth of the whole expression on this route.
    truth: bool,
    /// Last evaluated leaf.
    terminal_leaf: usize,
    /// Last evaluated leaf truth.
    terminal_outcome: bool,
}

/// Plans every full prefix route for one logical condition.
fn plan_full_paths(condition: &Expr) -> LogicalPathPlan {
    let mut leaf_spans = Vec::new();
    let mut truncated = false;
    let mut cursor = 0;
    let mut paths = full_paths(condition, &mut cursor, &mut leaf_spans, &mut truncated);
    if cursor > MAX_PATH_LEAVES {
        paths.clear();
        truncated = true;
    }
    LogicalPathPlan {
        leaf_spans,
        paths,
        truncated,
    }
}

/// Returns every full prefix route through one logical subtree.
fn full_paths(
    condition: &Expr,
    cursor: &mut usize,
    leaf_spans: &mut Vec<Span>,
    truncated: &mut bool,
) -> Vec<FullPath> {
    match &condition.kind {
        ExprKind::BinaryOp { left, op, right } if matches!(op, BinOp::And | BinOp::Or) => {
            let left_paths = full_paths(left, cursor, leaf_spans, truncated);
            let right_paths = full_paths(right, cursor, leaf_spans, truncated);
            let is_and = matches!(op, BinOp::And);
            let mut paths = Vec::new();
            for left_path in left_paths {
                let short_circuits = if is_and { !left_path.truth } else { left_path.truth };
                if short_circuits {
                    paths.push(left_path);
                } else {
                    for right_path in &right_paths {
                        let mut conditions = left_path.conditions.clone();
                        conditions.extend_from_slice(&right_path.conditions);
                        paths.push(FullPath {
                            conditions,
                            truth: right_path.truth,
                            terminal_leaf: right_path.terminal_leaf,
                            terminal_outcome: right_path.terminal_outcome,
                        });
                        if paths.len() >= MAX_LOGICAL_PATHS {
                            *truncated = true;
                            break;
                        }
                    }
                }
                if paths.len() >= MAX_LOGICAL_PATHS {
                    *truncated = true;
                    break;
                }
            }
            paths
        }
        ExprKind::Not(inner) => full_paths(inner, cursor, leaf_spans, truncated)
            .into_iter()
            .map(|path| FullPath {
                truth: !path.truth,
                ..path
            })
            .collect(),
        _ => {
            let leaf = *cursor;
            *cursor += 1;
            leaf_spans.push(condition.span);
            vec![
                FullPath {
                    conditions: vec![(leaf, true)],
                    truth: true,
                    terminal_leaf: leaf,
                    terminal_outcome: true,
                },
                FullPath {
                    conditions: vec![(leaf, false)],
                    truth: false,
                    terminal_leaf: leaf,
                    terminal_outcome: false,
                },
            ]
        }
    }
}

/// Returns whether a condition contains a logical operator (`&&`, `||` or `!`).
fn condition_has_logical_operator(condition: &Expr) -> bool {
    matches!(
        &condition.kind,
        ExprKind::BinaryOp {
            op: BinOp::And | BinOp::Or,
            ..
        } | ExprKind::Not(_)
    )
}

/// Packs one leaf outcome into its two path-state bits.
fn path_state_bits(leaf: usize, outcome: bool) -> u64 {
    let evaluated = 1u64 << (2 * leaf);
    let truth = if outcome {
        1u64 << (2 * leaf + 1)
    } else {
        0
    };
    evaluated | truth
}

/// Builds the state mask for one full route.
fn path_mask(conditions: &[(usize, bool)]) -> u64 {
    conditions
        .iter()
        .fold(0u64, |mask, (leaf, outcome)| {
            mask | path_state_bits(*leaf, *outcome)
        })
}

/// Renders one full route's leaf outcomes and terminal position.
fn path_label(path: &FullPath, leaf_spans: &[Span]) -> String {
    let conditions = path
        .conditions
        .iter()
        .map(|(leaf, outcome)| format!("#{leaf}={}", if *outcome { "T" } else { "F" }))
        .collect::<Vec<_>>()
        .join(",");
    let (line, column) = leaf_spans
        .get(path.terminal_leaf)
        .map(|span| (span.line, span.col))
        .unwrap_or((0, 0));
    format!("{conditions}@{line}:{column}")
}

/// Adds one leaf outcome to the condition's path-state word.
fn update_path_state(
    ctx: &mut LoweringContext<'_, '_>,
    state_name: &str,
    bits: u64,
    span: Span,
) {
    let state = ctx.load_local(state_name, Some(span));
    let bits = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(bits as i64)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    );
    let updated = ctx.emit_value(
        Op::IBitOr,
        vec![state.value, bits.value],
        None,
        PhpType::Int,
        Op::IBitOr.default_effects(),
        Some(span),
    );
    ctx.store_local(state_name, updated, PhpType::Int, Some(span));
}

/// Reserves binary-node continuations in AST preorder, allocating their blocks in leaf order.
///
/// Each right subtree starts after every leaf of its left subtree. Planning before lowering
/// keeps that order even when a subtree mixes And, Or and Not.
fn plan_condition_blocks(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    continuations: &mut Vec<Option<BlockId>>,
) {
    match &condition.kind {
        ExprKind::BinaryOp { left, op, right } if matches!(op, BinOp::And | BinOp::Or) => {
            let index = continuations.len();
            continuations.push(None);
            plan_condition_blocks(ctx, left, continuations);
            continuations[index] = Some(ctx.builder.create_named_block("condition.rhs", Vec::new()));
            plan_condition_blocks(ctx, right, continuations);
        }
        ExprKind::Not(inner) => plan_condition_blocks(ctx, inner, continuations),
        _ => {}
    }
}

/// Emits the logical AST against its preplanned continuations at its original expression depth.
fn lower_condition_tree(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_target: BlockId,
    else_target: BlockId,
    continuations: &mut impl Iterator<Item = BlockId>,
    state_name: Option<&str>,
    leaf_index: &mut usize,
    then_blocks: &mut Vec<BlockId>,
    else_blocks: &mut Vec<BlockId>,
) {
    ctx.expression_depth += 1;
    match &condition.kind {
        ExprKind::BinaryOp { left, op, right } if matches!(op, BinOp::And | BinOp::Or) => {
            let rhs = continuations.next().expect("logical continuation was planned");
            let is_and = matches!(op, BinOp::And);
            lower_condition_tree(
                ctx,
                left,
                if is_and { rhs } else { then_target },
                if is_and { else_target } else { rhs },
                continuations,
                state_name,
                leaf_index,
                then_blocks,
                else_blocks,
            );
            ctx.builder.position_at_end(rhs);
            lower_condition_tree(
                ctx,
                right,
                then_target,
                else_target,
                continuations,
                state_name,
                leaf_index,
                then_blocks,
                else_blocks,
            );
        }
        ExprKind::Not(inner) => lower_condition_tree(
            ctx,
            inner,
            else_target,
            then_target,
            continuations,
            state_name,
            leaf_index,
            then_blocks,
            else_blocks,
        ),
        _ => {
            let index = *leaf_index;
            *leaf_index += 1;
            let value = lower_expr_inner(ctx, condition);
            let value = ctx.truthy_consuming(value, Some(condition.span));
            if condition.span.source_path().is_some() && condition.span.line > 0 {
                if let Some(block) = ctx.builder.insertion_block() {
                    ctx.builder.record_branch_site(crate::ir::BranchSite {
                        span: condition.span,
                        kind: crate::ir::BranchSiteKind::Conditional { block },
                    });
                }
            }
            if let Some(state_name) = state_name {
                let then_state = ctx
                    .builder
                    .create_named_block("condition.path.then", Vec::new());
                let else_state = ctx
                    .builder
                    .create_named_block("condition.path.else", Vec::new());
                ctx.builder.terminate(Terminator::CondBr {
                    cond: value.value,
                    then_target: then_state,
                    then_args: Vec::new(),
                    else_target: else_state,
                    else_args: Vec::new(),
                });
                then_blocks.push(then_state);
                else_blocks.push(else_state);
                ctx.builder.position_at_end(then_state);
                update_path_state(
                    ctx,
                    state_name,
                    path_state_bits(index, true),
                    condition.span,
                );
                branch_to(ctx, then_target);
                ctx.builder.position_at_end(else_state);
                update_path_state(
                    ctx,
                    state_name,
                    path_state_bits(index, false),
                    condition.span,
                );
                branch_to(ctx, else_target);
            } else {
                ctx.builder.terminate(Terminator::CondBr {
                    cond: value.value,
                    then_target,
                    then_args: Vec::new(),
                    else_target,
                    else_args: Vec::new(),
                });
            }
        }
    }
    ctx.expression_depth -= 1;
}
