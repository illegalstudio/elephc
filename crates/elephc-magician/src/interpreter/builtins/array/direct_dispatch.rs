//! Purpose:
//! Area-level direct dispatch for array builtins declared in the eval registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::EvalDirectHook::call()`.
//!
//! Key details:
//! - Dispatch stays thin and routes every builtin through its leaf adapter.

use super::super::super::*;

/// Routes direct expression-level array builtin calls through per-builtin leaf adapters.
pub(in crate::interpreter) fn eval_builtin_array_declared_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "array_sum" => super::array_sum::eval_array_sum_declared_call(args, context, scope, values),
        "array_product" => super::array_product::eval_array_product_declared_call(args, context, scope, values),
        "array_chunk" => super::array_chunk::eval_array_chunk_declared_call(args, context, scope, values),
        "array_column" => super::array_column::eval_array_column_declared_call(args, context, scope, values),
        "array_combine" => super::array_combine::eval_array_combine_declared_call(args, context, scope, values),
        "array_diff" => super::array_diff::eval_array_diff_declared_call(args, context, scope, values),
        "array_diff_key" => super::array_diff_key::eval_array_diff_key_declared_call(args, context, scope, values),
        "array_fill" => super::array_fill::eval_array_fill_declared_call(args, context, scope, values),
        "array_fill_keys" => super::array_fill_keys::eval_array_fill_keys_declared_call(args, context, scope, values),
        "array_filter" => super::array_filter::eval_array_filter_declared_call(args, context, scope, values),
        "array_intersect" => super::array_intersect::eval_array_intersect_declared_call(args, context, scope, values),
        "array_intersect_key" => super::array_intersect_key::eval_array_intersect_key_declared_call(args, context, scope, values),
        "array_map" => super::array_map::eval_array_map_declared_call(args, context, scope, values),
        "array_merge" => super::array_merge::eval_array_merge_declared_call(args, context, scope, values),
        "array_reduce" => super::array_reduce::eval_array_reduce_declared_call(args, context, scope, values),
        "iterator_apply" => super::iterator_apply::eval_iterator_apply_declared_call(args, context, scope, values),
        "iterator_count" => super::iterator_count::eval_iterator_count_declared_call(args, context, scope, values),
        "iterator_to_array" => super::iterator_to_array::eval_iterator_to_array_declared_call(args, context, scope, values),
        "array_flip" => super::array_flip::eval_array_flip_declared_call(args, context, scope, values),
        "array_key_exists" => super::array_key_exists::eval_array_key_exists_declared_call(args, context, scope, values),
        "array_pad" => super::array_pad::eval_array_pad_declared_call(args, context, scope, values),
        "array_keys" => super::array_keys::eval_array_keys_declared_call(args, context, scope, values),
        "array_rand" => super::array_rand::eval_array_rand_declared_call(args, context, scope, values),
        "array_reverse" => super::array_reverse::eval_array_reverse_declared_call(args, context, scope, values),
        "array_search" => super::array_search::eval_array_search_declared_call(args, context, scope, values),
        "in_array" => super::in_array::eval_in_array_declared_call(args, context, scope, values),
        "array_slice" => super::array_slice::eval_array_slice_declared_call(args, context, scope, values),
        "array_unique" => super::array_unique::eval_array_unique_declared_call(args, context, scope, values),
        "array_values" => super::array_values::eval_array_values_declared_call(args, context, scope, values),
        "count" => super::count::eval_count_declared_call(args, context, scope, values),
        "range" => super::range::eval_range_declared_call(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
