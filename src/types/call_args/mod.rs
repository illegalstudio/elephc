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
