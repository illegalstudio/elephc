mod expr;
mod loop_exit;
mod statements;

pub(crate) use expr::{
    callable_target_effect, expr_has_side_effects, prune_expr,
};
pub(crate) use statements::prune_block;
