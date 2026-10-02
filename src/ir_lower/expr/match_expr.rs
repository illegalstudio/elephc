//! Purpose:
//! Lazy match-expression lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a match expression with lazy arm-result evaluation.
pub(super) fn lower_match(
    ctx: &mut LoweringContext<'_, '_>,
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    expr: &Expr,
) -> LoweredValue {
    let subject = lower_expr(ctx, subject);
    let result_type = match_merge_result_type(ctx, arms, default, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx.builder.create_named_block("match.merge", Vec::new());
    // Every arm starts from the facts of the dispatch path that reaches it: a result arm's
    // assignments hold neither for the arm tests lowered after it nor below the merge.
    let mut join = crate::ir_lower::stmt::ExprBranchJoin::at_split(ctx);

    for (conditions, result) in arms {
        let result_block = ctx.builder.create_named_block("match.result", Vec::new());
        let mut fallthrough = ctx.builder.insertion_block();
        // A condition list reaches the result block once per condition, each edge carrying the
        // facts that condition left: `($v = null), ($v = 1) => …` enters with `$v` null on the
        // first edge and int on the second. Those edges are joined like `if` arms, so the result
        // (and everything below the merge) reads `$v` through a type every edge can represent.
        let mut hits =
            (conditions.len() > 1).then(|| crate::ir_lower::stmt::ExprBranchJoin::at_split(ctx));
        for condition in conditions {
            let next_test = ctx.builder.create_named_block("match.next", Vec::new());
            let condition = lower_expr(ctx, condition);
            let matched = ctx.emit_value(
                Op::StrictEq,
                vec![subject.value, condition.value],
                None,
                PhpType::Bool,
                Op::StrictEq.default_effects(),
                Some(expr.span),
            );
            let hit_block = match hits {
                Some(_) => ctx.builder.create_named_block("match.hit", Vec::new()),
                None => result_block,
            };
            ctx.builder.terminate(Terminator::CondBr {
                cond: matched.value,
                then_target: hit_block,
                then_args: Vec::new(),
                else_target: next_test,
                else_args: Vec::new(),
            });
            if let Some(hits) = hits.as_mut() {
                ctx.builder.position_at_end(hit_block);
                hits.leave_arm(ctx);
            }
            ctx.builder.position_at_end(next_test);
            fallthrough = Some(next_test);
        }
        let dispatch = crate::ir_lower::stmt::ExprBranchJoin::at_split(ctx);
        match hits {
            Some(hits) => hits.finish(ctx, result_block, expr.span),
            None => ctx.builder.position_at_end(result_block),
        }
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), result, expr.span);
        join.leave_arm(ctx);
        dispatch.enter_arm(ctx);
        if let Some(fallthrough) = fallthrough {
            ctx.builder.position_at_end(fallthrough);
        }
    }
    if let Some(default) = default {
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), default, expr.span);
        join.leave_arm(ctx);
    } else if !ctx.builder.insertion_block_is_terminated() {
        let message = ctx.intern_string("Fatal error: unhandled match case\n");
        ctx.builder.terminate(Terminator::Fatal { message });
    }
    join.finish(ctx, merge, expr.span);
    take_owned_temp(ctx, &temp_name, expr.span)
}

