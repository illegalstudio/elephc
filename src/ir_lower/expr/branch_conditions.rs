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
pub(crate) fn lower_condition(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_target: BlockId,
    else_target: BlockId,
) {
    let mut continuations = Vec::new();
    plan_condition_blocks(ctx, condition, &mut continuations);
    let mut continuations = continuations.into_iter()
        .map(|block| block.expect("every logical node has a continuation block"));
    lower_condition_tree(ctx, condition, then_target, else_target, &mut continuations);
    debug_assert!(continuations.next().is_none());
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
) {
    ctx.expression_depth += 1;
    match &condition.kind {
        ExprKind::BinaryOp { left, op, right } if matches!(op, BinOp::And | BinOp::Or) => {
            let rhs = continuations.next().expect("logical continuation was planned");
            let is_and = matches!(op, BinOp::And);
            lower_condition_tree(
                ctx, left, if is_and { rhs } else { then_target },
                if is_and { else_target } else { rhs }, continuations,
            );
            ctx.builder.position_at_end(rhs);
            lower_condition_tree(ctx, right, then_target, else_target, continuations);
        }
        ExprKind::Not(inner) =>
            lower_condition_tree(ctx, inner, else_target, then_target, continuations),
        _ => {
            let value = lower_expr_inner(ctx, condition);
            let value = ctx.truthy_consuming(value, Some(condition.span));
            ctx.builder.terminate(Terminator::CondBr {
                cond: value.value,
                then_target,
                then_args: Vec::new(),
                else_target,
                else_args: Vec::new(),
            });
        }
    }
    ctx.expression_depth -= 1;
}
