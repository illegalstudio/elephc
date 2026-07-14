//! Purpose:
//! Area-level evaluated-argument dispatch for array builtins declared in the eval registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::EvalValuesHook::call()`.
//!
//! Key details:
//! - Dispatch stays thin and routes every builtin through its leaf adapter.

use super::super::super::*;

/// Routes evaluated-argument array builtin calls through per-builtin leaf adapters.
pub(in crate::interpreter) fn eval_array_declared_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "array_sum" => super::array_sum::eval_array_sum_declared_values_result(evaluated_args, context, values),
        "array_product" => super::array_product::eval_array_product_declared_values_result(evaluated_args, context, values),
        "array_chunk" => super::array_chunk::eval_array_chunk_declared_values_result(evaluated_args, context, values),
        "array_column" => super::array_column::eval_array_column_declared_values_result(evaluated_args, context, values),
        "array_combine" => super::array_combine::eval_array_combine_declared_values_result(evaluated_args, context, values),
        "array_diff" => super::array_diff::eval_array_diff_declared_values_result(evaluated_args, context, values),
        "array_diff_key" => super::array_diff_key::eval_array_diff_key_declared_values_result(evaluated_args, context, values),
        "array_fill" => super::array_fill::eval_array_fill_declared_values_result(evaluated_args, context, values),
        "array_fill_keys" => super::array_fill_keys::eval_array_fill_keys_declared_values_result(evaluated_args, context, values),
        "array_filter" => super::array_filter::eval_array_filter_declared_values_result(evaluated_args, context, values),
        "array_intersect" => super::array_intersect::eval_array_intersect_declared_values_result(evaluated_args, context, values),
        "array_intersect_key" => super::array_intersect_key::eval_array_intersect_key_declared_values_result(evaluated_args, context, values),
        "array_map" => super::array_map::eval_array_map_declared_values_result(evaluated_args, context, values),
        "array_merge" => super::array_merge::eval_array_merge_declared_values_result(evaluated_args, context, values),
        "array_reduce" => super::array_reduce::eval_array_reduce_declared_values_result(evaluated_args, context, values),
        "iterator_apply" => super::iterator_apply::eval_iterator_apply_declared_values_result(evaluated_args, context, values),
        "iterator_count" => super::iterator_count::eval_iterator_count_declared_values_result(evaluated_args, context, values),
        "iterator_to_array" => super::iterator_to_array::eval_iterator_to_array_declared_values_result(evaluated_args, context, values),
        "array_flip" => super::array_flip::eval_array_flip_declared_values_result(evaluated_args, context, values),
        "array_key_exists" => super::array_key_exists::eval_array_key_exists_declared_values_result(evaluated_args, context, values),
        "array_pad" => super::array_pad::eval_array_pad_declared_values_result(evaluated_args, context, values),
        "array_keys" => super::array_keys::eval_array_keys_declared_values_result(evaluated_args, context, values),
        "array_rand" => super::array_rand::eval_array_rand_declared_values_result(evaluated_args, context, values),
        "array_reverse" => super::array_reverse::eval_array_reverse_declared_values_result(evaluated_args, context, values),
        "array_search" => super::array_search::eval_array_search_declared_values_result(evaluated_args, context, values),
        "in_array" => super::in_array::eval_in_array_declared_values_result(evaluated_args, context, values),
        "array_slice" => super::array_slice::eval_array_slice_declared_values_result(evaluated_args, context, values),
        "array_unique" => super::array_unique::eval_array_unique_declared_values_result(evaluated_args, context, values),
        "array_values" => super::array_values::eval_array_values_declared_values_result(evaluated_args, context, values),
        "count" => super::count::eval_count_declared_values_result(evaluated_args, context, values),
        "range" => super::range::eval_range_declared_values_result(evaluated_args, context, values),
        "array_walk" => super::array_walk::eval_array_walk_declared_values_result(evaluated_args, context, values),
        "array_pop" => super::array_pop::eval_array_pop_declared_values_result(evaluated_args, context, values),
        "array_shift" => super::array_shift::eval_array_shift_declared_values_result(evaluated_args, context, values),
        "array_push" => super::array_push::eval_array_push_declared_values_result(evaluated_args, context, values),
        "array_unshift" => super::array_unshift::eval_array_unshift_declared_values_result(evaluated_args, context, values),
        "array_splice" => super::array_splice::eval_array_splice_declared_values_result(evaluated_args, context, values),
        "arsort" => super::arsort::eval_arsort_declared_values_result(evaluated_args, context, values),
        "asort" => super::asort::eval_asort_declared_values_result(evaluated_args, context, values),
        "krsort" => super::krsort::eval_krsort_declared_values_result(evaluated_args, context, values),
        "ksort" => super::ksort::eval_ksort_declared_values_result(evaluated_args, context, values),
        "natcasesort" => super::natcasesort::eval_natcasesort_declared_values_result(evaluated_args, context, values),
        "natsort" => super::natsort::eval_natsort_declared_values_result(evaluated_args, context, values),
        "rsort" => super::rsort::eval_rsort_declared_values_result(evaluated_args, context, values),
        "shuffle" => super::shuffle::eval_shuffle_declared_values_result(evaluated_args, context, values),
        "sort" => super::sort::eval_sort_declared_values_result(evaluated_args, context, values),
        "uasort" => super::uasort::eval_uasort_declared_values_result(evaluated_args, context, values),
        "uksort" => super::uksort::eval_uksort_declared_values_result(evaluated_args, context, values),
        "usort" => super::usort::eval_usort_declared_values_result(evaluated_args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
