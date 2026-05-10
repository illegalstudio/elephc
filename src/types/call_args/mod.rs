//! Purpose:
//! Exposes the shared semantic planner for function-like call arguments.
//! Normalizes positional, named, variadic, and static spread arguments for checker and codegen consumers.
//!
//! Called from:
//! - `crate::types::checker::functions`
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - All call surfaces must use this planner so PHP named/spread semantics stay consistent.

mod matching;
mod plan;
mod planner;
mod static_spread;

pub(crate) use matching::{named_param_index, regular_param_count};
pub(crate) use plan::{
    CallArgPlan, CallArgPlanError, PlannedRegularArg, PlannedSourceValue, SpreadBoundsCheck,
};
pub(crate) use planner::{plan_call_args, plan_call_args_with_regular_param_count};
pub(crate) use static_spread::{expand_static_assoc_spread_args, has_named_args};
