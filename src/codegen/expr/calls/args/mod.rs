//! Purpose:
//! Coordinates call-argument lowering from semantic call plans into ABI-ready temporary values.
//! Re-exports helpers for named, spread, variadic, and array-element argument paths.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - Source-order side effects and ABI-order materialization are deliberately separated across this module tree.

mod assoc_variadic;
mod array_elements;
mod common;
mod emit;
mod named;
mod normalize;
mod spread;
mod spread_checks;
mod variadic;

use crate::parser::ast::Expr;
use crate::types::call_args::SpreadBoundsCheck;
use crate::types::PhpType;

pub(crate) use assoc_variadic::emit_loaded_assoc_variadic_array_arg;
pub(crate) use array_elements::{
    array_element_stride, emit_hash_lookup_for_param_or_index, load_array_element_to_result,
    push_loaded_array_element_arg, push_loaded_hash_value_arg, push_loaded_hash_value_ref_arg,
};
pub(crate) use common::{
    coerce_current_value_to_target, declared_target_ty, emit_ref_arg_variable_address,
    push_arg_value, push_current_result_ref_arg_address, push_expr_arg,
    push_non_variable_ref_arg_address, release_preserved_mixed_after_arg_coercion,
};
pub(crate) use emit::emit_pushed_call_args;
pub(crate) use named::pushed_temp_bytes;
pub(crate) use normalize::{
    has_named_args, named_call_arg_temp_name, named_call_prefix_temp_name,
    normalize_builtin_call_args_with_checks, normalize_named_call_args_with_checks,
    preevaluate_named_call_args_to_temps, regular_param_count,
};
pub(crate) use spread_checks::emit_spread_length_checks;
use array_elements::spread_source_elem_ty;
use spread_checks::{emit_array_length_bounds_check, emit_named_spread_length_abort};
use variadic::{store_current_array_element, variadic_container_elem_ty};
pub(crate) use variadic::emit_empty_variadic_array_arg;

/// Holds normalized argument expressions paired with their required spread-length validation checks.
/// Produced by `normalize_named_call_args_with_checks` and `normalize_builtin_call_args_with_checks`.
pub(crate) struct NormalizedCallArgs {
    pub(crate) args: Vec<Expr>,
    pub(crate) spread_length_checks: Vec<SpreadBoundsCheck>,
}

/// Holds the decomposed call-argument state for positional (non-named) calls.
/// Tracks regular arguments, variadic arguments, spread arguments, and metadata needed for ABI materialization.
/// Produced by `prepare_call_args`.
pub(crate) struct PreparedCallArgs {
    pub(crate) all_args: Vec<Expr>,
    pub(crate) variadic_args: Vec<Expr>,
    pub(crate) spread_arg: Option<Expr>,
    pub(crate) spread_at_index: usize,
    pub(crate) regular_param_count: usize,
    pub(crate) is_variadic: bool,
    pub(crate) spread_into_named: bool,
}

/// Holds the emitted call-argument state after ABI materialization.
/// `arg_types` lists the runtime PHP type of each pushed argument in order.
/// `source_temp_bytes` tracks total stack bytes used for source temporaries (populated by named-arg lowering, zero elsewhere).
pub(crate) struct EmittedCallArgs {
    pub(crate) arg_types: Vec<PhpType>,
    pub(crate) source_temp_bytes: usize,
}
