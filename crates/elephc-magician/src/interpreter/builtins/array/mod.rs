//! Purpose:
//! Per-builtin declarations for array and collection functions migrated to the
//! eval builtin registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.

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
mod array_pop;
mod array_pad;
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
mod in_array;
mod iterator_apply;
mod iterator_count;
mod iterator_to_array;
mod krsort;
mod ksort;
mod mutating;
mod natcasesort;
mod natsort;
mod non_mutating;
mod range;
mod rsort;
mod shuffle;
mod sort;
mod uasort;
mod uksort;
mod usort;

pub(in crate::interpreter) use mutating::*;
pub(in crate::interpreter) use non_mutating::*;
