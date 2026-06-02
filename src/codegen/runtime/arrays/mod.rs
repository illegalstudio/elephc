//! Purpose:
//! Collects array, hash, heap, GC, iterable, and Mixed runtime emitters.
//! The module owns re-export wiring for helpers that are emitted by the runtime orchestrator.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` during the array runtime section.
//!
//! Key details:
//! - Array, hash, heap, GC, and Mixed helpers must preserve runtime layout, refcounts, and COW rules before mutating shared storage.

mod array_chunk;
mod array_chunk_refcounted;
mod array_column;
mod array_column_mixed;
mod array_column_ref;
mod array_column_str;
mod array_combine;
mod array_combine_refcounted;
mod array_clone_shallow;
mod array_diff;
mod array_diff_refcounted;
mod array_diff_key;
mod array_ensure_unique;
mod array_fill;
mod array_fill_keys;
mod array_fill_keys_refcounted;
mod array_fill_refcounted;
mod array_filter;
mod array_filter_refcounted;
mod array_flip;
mod array_flip_string;
mod array_free_deep;
mod array_grow;
mod array_hash_union;
mod array_intersect;
mod array_intersect_refcounted;
mod array_intersect_key;
mod array_key_exists;
mod array_map;
mod array_map_mixed;
mod array_map_str;
mod array_merge;
mod array_merge_into;
mod array_merge_into_refcounted;
mod array_merge_refcounted;
mod array_new;
mod array_pad;
mod array_pad_refcounted;
mod array_product;
mod array_push_int;
mod array_push_refcounted;
mod array_push_str;
mod array_rand;
mod random_u32;
mod random_uniform;
mod array_reduce;
mod array_reverse;
mod array_reverse_refcounted;
mod array_search;
mod array_shift;
mod array_slice;
mod array_slice_refcounted;
mod array_splice;
mod array_splice_refcounted;
mod array_sum;
mod array_to_mixed;
mod array_union;
mod array_unique;
mod array_unique_refcounted;
mod array_unshift;
mod array_walk;
mod asort;
mod decref_any;
mod decref_array;
mod decref_hash;
mod decref_mixed;
mod decref_object;
mod gc_collect_cycles;
mod gc_collect_cycles_x86_64;
mod gc_mark_reachable;
mod gc_note_child_ref;
mod hash_count;
mod hash_append;
mod hash_clone_shallow;
mod hash_fnv1a;
mod hash_free_deep;
mod hash_get;
mod hash_grow;
mod hash_array_union;
mod hash_key_eq;
mod hash_key_hash;
mod hash_normalize_key;
mod hash_may_have_cyclic_values;
mod hash_ensure_unique;
mod hash_insert_owned;
mod hash_iter;
mod hash_new;
mod hash_set;
mod hash_to_mixed;
mod hash_union;
mod heap_alloc;
mod heap_debug_check_live;
mod heap_debug_fail;
mod heap_debug_report;
mod heap_debug_validate_free_list;
mod heap_kind;
mod heap_free;
mod ksort;
mod natsort;
mod object_free_deep;
mod range;
mod incref;
mod iterable_unsupported_kind;
mod iterable_write_stdout;
mod mixed_from_value;
mod mixed_instanceof;
mod mixed_cast_bool;
mod mixed_cast_float;
mod mixed_cast_int;
mod mixed_cast_string;
mod mixed_free_deep;
mod mixed_count;
mod mixed_is_empty;
mod mixed_numeric_binops;
mod mixed_strict_eq;
mod mixed_unbox;
mod mixed_write_stdout;
mod refcount;
mod shuffle;
mod sort_int;
mod sort_str;
mod undefined_array_key_warning;
mod usort;
mod value_error;

pub use array_chunk::emit_array_chunk;
/// Emit array chunk helper (split array into chunks).
pub use array_chunk_refcounted::emit_array_chunk_refcounted;
/// Emit refcounted array chunk helper.
pub use array_column::emit_array_column;
/// Emit array column extraction helper.
pub use array_column_mixed::emit_array_column_mixed;
/// Emit Mixed-type array column helper.
pub use array_column_ref::emit_array_column_ref;
/// Emit refcounted array column helper.
pub use array_column_str::emit_array_column_str;
/// Emit string-only array column helper.
pub use array_combine::emit_array_combine;
/// Emit array combine (keys + values) helper.
pub use array_combine_refcounted::emit_array_combine_refcounted;
/// Emit refcounted array combine helper.
pub use array_clone_shallow::emit_array_clone_shallow;
/// Emit shallow array clone helper.
pub use array_diff::emit_array_diff;
/// Emit array difference helper.
pub use array_diff_refcounted::emit_array_diff_refcounted;
/// Emit refcounted array difference helper.
pub use array_diff_key::emit_array_diff_key;
/// Emit array difference by key helper.
pub use array_ensure_unique::emit_array_ensure_unique;
/// Emit array uniqueness enforcement helper.
pub use array_fill::emit_array_fill;
/// Emit array fill helper.
pub use array_fill_keys::emit_array_fill_keys;
/// Emit array fill with keys helper.
pub use array_fill_keys_refcounted::emit_array_fill_keys_refcounted;
/// Emit refcounted array fill with keys helper.
pub use array_fill_refcounted::emit_array_fill_refcounted;
/// Emit refcounted array fill helper.
pub use array_filter::emit_array_filter;
/// Emit array filter helper.
pub use array_filter_refcounted::emit_array_filter_refcounted;
/// Emit refcounted array filter helper.
pub use array_flip::emit_array_flip;
/// Emit array flip helper.
pub use array_flip_string::emit_array_flip_string;
/// Emit string-only array flip helper.
pub use array_free_deep::emit_array_free_deep;
/// Emit deep array free helper.
pub use array_grow::emit_array_grow;
/// Emit array grow helper.
pub use array_hash_union::emit_array_hash_union;
/// Emit array hash union helper.
pub use array_intersect::emit_array_intersect;
/// Emit array intersection helper.
pub use array_intersect_refcounted::emit_array_intersect_refcounted;
/// Emit array intersection by key helper.
pub use array_intersect_key::emit_array_intersect_key;
/// Emit array key existence check helper.
pub use array_key_exists::emit_array_key_exists;
/// Emit array map helper.
pub use array_map::emit_array_map;
/// Emit mixed-result array map helper.
pub use array_map_mixed::emit_array_map_mixed;
/// Emit string-returning array map helpers.
pub use array_map_str::{emit_array_map_str, emit_array_map_str_owned};
/// Emit array merge helper.
pub use array_merge::emit_array_merge;
/// Emit array merge-into helper.
pub use array_merge_into::emit_array_merge_into;
pub use array_merge_into_refcounted::emit_array_merge_into_refcounted;
/// Emit refcounted merge-into helper.
pub use array_merge_refcounted::emit_array_merge_refcounted;
/// Emit refcounted array merge helper.
pub use array_new::emit_array_new;
/// Emit new empty array helper.
pub use array_pad::emit_array_pad;
/// Emit array padding helper.
pub use array_pad_refcounted::emit_array_pad_refcounted;
/// Emit refcounted array pad helper.
pub use array_product::emit_array_product;
/// Emit array product helper.
pub use array_push_int::emit_array_push_int;
/// Emit integer-optimized array push helper.
pub use array_push_refcounted::emit_array_push_refcounted;
/// Emit refcounted array push helper.
pub use array_push_str::emit_array_push_str;
/// Emit string-optimized array push helper.
pub use array_rand::emit_array_rand;
/// Emit random array element helper.
pub use random_u32::emit_random_u32;
/// Emit 32-bit random unsigned integer helper.
pub use random_uniform::emit_random_uniform;
/// Emit uniform random integer helper.
pub use array_reduce::emit_array_reduce;
/// Emit array reduce helper.
pub use array_reverse::emit_array_reverse;
/// Emit array reverse helper.
pub use array_reverse_refcounted::emit_array_reverse_refcounted;
/// Emit refcounted array reverse helper.
pub use array_search::emit_array_search;
/// Emit array search helper.
pub use array_shift::emit_array_shift;
/// Emit array shift (remove first) helper.
pub use array_slice::emit_array_slice;
/// Emit array slice extraction helper.
pub use array_slice_refcounted::emit_array_slice_refcounted;
/// Emit refcounted array slice helper.
pub use array_splice::emit_array_splice;
/// Emit array splice helper.
pub use array_splice_refcounted::emit_array_splice_refcounted;
/// Emit refcounted array splice helper.
pub use array_sum::emit_array_sum;
/// Emit array sum helper.
pub use array_to_mixed::emit_array_to_mixed;
/// Emit array-to-Mixed conversion helper.
pub use array_union::emit_array_union;
/// Emit array union helper.
pub use array_unique::emit_array_unique;
/// Emit array unique helper.
pub use array_unique_refcounted::emit_array_unique_refcounted;
/// Emit refcounted array unique helper.
pub use array_unshift::emit_array_unshift;
/// Emit array unshift (prepend) helper.
pub use array_walk::emit_array_walk;
/// Emit array walk helper.
pub use asort::emit_asort;
/// Emit associative sort helper.
pub use decref_any::emit_decref_any;
/// Emit generic reference decrement helper.
pub use decref_mixed::emit_decref_mixed;
/// Emit Mixed reference decrement helper.
pub use hash_count::emit_hash_count;
/// Emit hash count helper.
pub use hash_append::emit_hash_append;
/// Emit hash append helper.
pub use hash_clone_shallow::emit_hash_clone_shallow;
/// Emit shallow hash clone helper.
pub use gc_collect_cycles::emit_gc_collect_cycles;
/// Emit garbage collection cycle collector.
pub use gc_mark_reachable::emit_gc_mark_reachable;
/// Emit GC mark reachable helper.
pub use gc_note_child_ref::emit_gc_note_child_ref;
/// Emit GC note child reference helper.
pub use hash_fnv1a::emit_hash_fnv1a;
/// Emit FNV-1a hash helper.
pub use hash_free_deep::emit_hash_free_deep;
/// Emit deep hash free helper.
pub use hash_get::emit_hash_get;
/// Emit hash get helper.
pub use hash_grow::emit_hash_grow;
/// Emit hash grow helper.
pub use hash_array_union::emit_hash_array_union;
/// Emit hash array union helper.
pub use hash_key_eq::emit_hash_key_eq;
/// Emit hash key equality check.
pub use hash_key_hash::emit_hash_key_hash;
/// Emit hash key hash computation.
pub use hash_normalize_key::emit_hash_normalize_key;
/// Emit hash key normalization helper.
pub use hash_may_have_cyclic_values::emit_hash_may_have_cyclic_values;
/// Emit cyclic value detection helper.
pub use hash_ensure_unique::emit_hash_ensure_unique;
/// Emit hash uniqueness enforcement helper.
pub use hash_insert_owned::emit_hash_insert_owned;
/// Emit owned hash insert helper.
pub use hash_iter::emit_hash_iter;
/// Emit hash iterator helper.
pub use hash_new::emit_hash_new;
/// Emit new hash helper.
pub use hash_set::emit_hash_set;
/// Emit hash set helper.
pub use hash_to_mixed::emit_hash_to_mixed;
/// Emit hash-to-Mixed conversion helper.
pub use hash_union::emit_hash_union;
/// Emit hash union helper.
pub use heap_alloc::emit_heap_alloc;
/// Emit heap allocation helper.
pub use heap_debug_check_live::emit_heap_debug_check_live;
/// Emit heap debug live check helper.
pub use heap_debug_fail::emit_heap_debug_fail;
/// Emit heap debug failure helper.
pub use heap_debug_report::emit_heap_debug_report;
/// Emit heap debug report helper.
pub use heap_debug_validate_free_list::emit_heap_debug_validate_free_list;
/// Emit heap free list validation helper.
pub use heap_kind::emit_heap_kind;
/// Emit heap kind check helper.
pub use heap_free::emit_heap_free;
/// Emit heap free helper.
pub use iterable_unsupported_kind::emit_iterable_unsupported_kind;
/// Emit unsupported iterable kind error helper.
pub use iterable_write_stdout::emit_iterable_write_stdout;
/// Emit iterable write to stdout helper.
pub use ksort::emit_ksort;
/// Emit key sort helper.
pub use natsort::emit_natsort;
/// Emit natural sort helper.
pub use mixed_from_value::emit_mixed_from_value;
/// Emit Mixed from value conversion helper.
pub use mixed_instanceof::emit_mixed_instanceof;
/// Emit Mixed instanceof check helper.
pub use mixed_cast_bool::emit_mixed_cast_bool;
/// Emit Mixed-to-boolean cast helper.
pub use mixed_cast_float::emit_mixed_cast_float;
/// Emit Mixed-to-float cast helper.
pub use mixed_cast_int::emit_mixed_cast_int;
/// Emit Mixed-to-integer cast helper.
pub use mixed_cast_string::emit_mixed_cast_string;
/// Emit Mixed-to-string cast helper.
pub use mixed_count::emit_mixed_count;
/// Emit Mixed count helper.
pub use mixed_free_deep::emit_mixed_free_deep;
/// Emit deep Mixed free helper.
pub use mixed_is_empty::emit_mixed_is_empty;
/// Emit Mixed emptiness check helper.
pub use mixed_numeric_binops::emit_mixed_numeric_binops;
/// Emit Mixed numeric binary operations helper.
pub use mixed_strict_eq::emit_mixed_strict_eq;
/// Emit Mixed strict equality check helper.
pub use mixed_unbox::emit_mixed_unbox;
/// Emit Mixed unbox helper.
pub use mixed_write_stdout::emit_mixed_write_stdout;
/// Emit Mixed write to stdout helper.
pub use object_free_deep::emit_object_free_deep;
/// Emit deep object free helper.
pub use range::emit_range;
/// Emit range helper.
pub use refcount::emit_refcount;
/// Emit reference count helper.
pub use shuffle::emit_shuffle;
/// Emit array shuffle helper.
pub use sort_int::emit_sort_int;
/// Emit string sort helper.
pub use sort_str::emit_sort_str;
/// Emit undefined integer array key warning helper.
pub use undefined_array_key_warning::emit_undefined_array_key_warning;
/// Emit user-defined sort helper.
pub use usort::emit_usort;
