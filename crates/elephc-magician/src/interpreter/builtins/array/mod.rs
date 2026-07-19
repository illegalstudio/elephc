//! Purpose:
//! Per-builtin declarations and eval adapters for array and collection functions.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!` and own the concrete
//!   direct or evaluated-argument adapter used by registry hooks.

mod array_chunk;
mod array_column;
mod array_combine;
mod array_diff;
mod array_diff_key;
mod array_fill;
mod array_fill_keys;
mod array_filter;
mod array_flip;
mod array_intersect;
mod array_intersect_key;
mod array_key_exists;
mod array_keys;
mod array_map;
mod array_merge;
mod array_pad;
mod array_pop;
mod array_product;
mod array_push;
mod array_rand;
mod array_reduce;
mod array_reverse;
mod array_search;
mod array_shift;
mod array_slice;
mod array_splice;
mod array_sum;
mod array_unique;
mod array_unshift;
mod array_values;
mod array_walk;
mod arsort;
mod asort;
mod count;
mod direct_dispatch;
mod in_array;
mod iterator_apply;
mod iterator_count;
mod iterator_to_array;
mod krsort;
mod ksort;
mod mutating_dispatch;
mod mutation;
mod natcasesort;
mod natsort;
mod range;
mod rsort;
mod shuffle;
mod sort;
mod uasort;
mod uksort;
mod usort;
mod values_dispatch;

pub(in crate::interpreter) use array_pop::eval_array_pop_shift_replacement;
pub(in crate::interpreter) use array_push::{
    eval_array_push_unshift_count_result, eval_array_push_unshift_replacement,
};
pub(in crate::interpreter) use array_splice::eval_array_splice_removed_and_replacement;
pub(in crate::interpreter) use array_walk::eval_array_walk_ref_result;
pub(in crate::interpreter) use direct_dispatch::eval_builtin_array_declared_call;
pub(in crate::interpreter) use mutating_dispatch::eval_builtin_array_mutating_declared_call;
pub(in crate::interpreter) use sort::eval_array_sort_replacement;
pub(in crate::interpreter) use usort::eval_user_sort_replacement;
pub(in crate::interpreter) use values_dispatch::eval_array_declared_values_result;
