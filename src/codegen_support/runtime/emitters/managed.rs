//! Purpose:
//! Emits runtime helpers for managed PHP values, containers, resources, objects, and buffers.
//!
//! Called from:
//! - `super::emit_runtime()` after the core runtime helpers.
//!
//! Key details:
//! - Keeps heap, GC, eval-scope, SPL, object, and buffer helpers in dependency order.

use super::super::{
    arrays, buffers, compare, eval_bridge, eval_scope, objects, resource_ids, spl,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::RuntimeFeatures;

/// Emits helpers for managed values and their container or object abstractions.
pub(super) fn emit_managed_runtime(emitter: &mut Emitter, features: RuntimeFeatures) {
    // Array runtime functions
    arrays::emit_heap_alloc(emitter);
    arrays::emit_heap_debug_fail(emitter);
    arrays::emit_heap_debug_check_live(emitter);
    arrays::emit_heap_debug_validate_free_list(emitter);
    arrays::emit_heap_debug_report(emitter);
    arrays::emit_heap_kind(emitter);
    arrays::emit_heap_free(emitter);
    arrays::emit_array_free_deep(emitter);
    arrays::emit_array_clone_shallow(emitter);
    arrays::emit_array_ensure_unique(emitter);
    arrays::emit_array_grow(emitter);
    arrays::emit_array_new(emitter);
    arrays::emit_array_push_int(emitter);
    arrays::emit_array_push_refcounted(emitter);
    arrays::emit_array_push_str(emitter);
    arrays::emit_array_set_int(emitter);
    arrays::emit_array_set_mixed(emitter);
    arrays::emit_array_set_mixed_key(emitter);
    arrays::emit_array_get_mixed_key(emitter);
    arrays::emit_array_set_refcounted(emitter);
    arrays::emit_array_set_str(emitter);
    arrays::emit_array_union(emitter);
    arrays::emit_array_hash_union(emitter);
    arrays::emit_hash_array_union(emitter);
    arrays::emit_random_u32(emitter);
    arrays::emit_random_uniform(emitter);
    arrays::emit_random_u64(emitter);
    arrays::emit_random_uniform64(emitter);
    arrays::emit_sort_int(emitter, false);
    arrays::emit_sort_int(emitter, true);
    arrays::emit_sort_str(emitter, false);
    arrays::emit_sort_str(emitter, true);
    arrays::emit_filter_truthy_predicates(emitter);
    arrays::emit_natcmp(emitter, "__rt_natcmp", false);
    arrays::emit_natcmp(emitter, "__rt_natcasecmp", true);
    arrays::emit_natsort_str(emitter);
    arrays::emit_hash_fnv1a(emitter);
    arrays::emit_hash_key_hash(emitter);
    arrays::emit_hash_key_eq(emitter);
    arrays::emit_hash_normalize_key(emitter);
    arrays::emit_hash_clone_shallow(emitter);
    arrays::emit_hash_ensure_unique(emitter);
    arrays::emit_hash_new(emitter);
    arrays::emit_hash_grow(emitter);
    arrays::emit_hash_may_have_cyclic_values(emitter);
    arrays::emit_hash_set(emitter);
    arrays::emit_hash_unset(emitter);
    arrays::emit_hash_append(emitter);
    arrays::emit_hash_insert_owned(emitter);
    arrays::emit_hash_get(emitter);
    arrays::emit_hash_iter(emitter);
    arrays::emit_hash_union(emitter);
    arrays::emit_hash_spread(emitter);
    arrays::emit_hash_to_mixed(emitter);
    arrays::emit_hash_sum_mixed(emitter);
    arrays::emit_hash_count(emitter);
    arrays::emit_hash_free_deep(emitter);
    arrays::emit_array_key_exists(emitter);
    arrays::emit_array_key_exists_mixed_key(emitter);
    arrays::emit_undefined_array_key_warning(emitter);
    arrays::emit_array_search(emitter);
    arrays::emit_in_array_mixed_int(emitter);
    arrays::emit_array_reverse(emitter);
    arrays::emit_array_reverse_str(emitter);
    arrays::emit_array_reverse_refcounted(emitter);
    arrays::emit_array_sum(emitter);
    arrays::emit_array_sum_mixed(emitter);
    arrays::emit_array_product(emitter);
    arrays::emit_array_shift(emitter);
    arrays::emit_array_unshift(emitter);
    arrays::emit_array_merge(emitter);
    arrays::emit_array_merge_refcounted(emitter);
    arrays::emit_array_slice(emitter);
    arrays::emit_array_slice_refcounted(emitter);
    arrays::emit_array_slice_to_hash(emitter);
    arrays::emit_array_chunk_to_hash(emitter);
    arrays::emit_range(emitter);
    arrays::emit_shuffle(emitter);
    arrays::emit_shuffle_str(emitter);
    arrays::emit_array_rand(emitter);
    arrays::emit_array_fill(emitter);
    arrays::emit_array_fill_assoc(emitter);
    arrays::emit_array_fill_refcounted(emitter);
    arrays::emit_array_fill_str(emitter);
    arrays::emit_array_pad(emitter);
    arrays::emit_array_pad_refcounted(emitter);
    arrays::emit_array_diff(emitter);
    arrays::emit_array_set_op_str(emitter, "__rt_array_diff_str", false);
    arrays::emit_array_set_op_str(emitter, "__rt_array_intersect_str", true);
    // php preserves the first operand's keys in all three value set operations, which a dense
    // indexed array cannot represent; these build the int-keyed hash php actually returns.
    arrays::emit_array_set_op_to_hash(
        emitter,
        "__rt_array_diff_to_hash",
        arrays::SetOpToHash::Diff,
    );
    arrays::emit_array_set_op_to_hash(
        emitter,
        "__rt_array_intersect_to_hash",
        arrays::SetOpToHash::Intersect,
    );
    arrays::emit_array_set_op_to_hash(
        emitter,
        "__rt_array_unique_to_hash",
        arrays::SetOpToHash::Unique,
    );
    arrays::emit_array_merge_str(emitter);
    arrays::emit_array_unique_str(emitter);
    arrays::emit_array_diff_refcounted(emitter);
    arrays::emit_array_is_list(emitter);
    arrays::emit_array_edge_key(emitter);
    arrays::emit_array_ptr_seek(emitter);
    arrays::emit_array_ptr_key(emitter);
    arrays::emit_array_ptr_value(emitter);
    arrays::emit_array_intersect(emitter);
    arrays::emit_array_intersect_refcounted(emitter);
    arrays::emit_array_flip(emitter);
    arrays::emit_array_count_values(emitter);
    arrays::emit_array_flip_string(emitter);
    arrays::emit_hash_flip(emitter);
    arrays::emit_hash_map(emitter);
    arrays::emit_array_combine(emitter);
    arrays::emit_array_combine_refcounted(emitter);
    arrays::emit_array_fill_keys(emitter);
    arrays::emit_array_fill_keys_refcounted(emitter);
    arrays::emit_array_chunk(emitter);
    arrays::emit_array_chunk_refcounted(emitter);
    arrays::emit_array_column(emitter);
    arrays::emit_array_column_mixed(emitter);
    arrays::emit_array_column_ref(emitter);
    arrays::emit_array_column_str(emitter);
    arrays::emit_array_splice(emitter);
    arrays::emit_array_splice_refcounted(emitter);
    arrays::emit_array_splice_insert(emitter);
    arrays::emit_array_splice_insert_refcounted(emitter);
    arrays::emit_array_splice_insert_boxed(emitter);
    arrays::emit_array_splice_insert_unboxed(emitter);
    arrays::emit_array_slice_str(emitter);
    arrays::emit_array_splice_str(emitter);
    arrays::emit_array_splice_insert_str(emitter);
    arrays::emit_array_diff_key(emitter);
    arrays::emit_array_intersect_key(emitter);
    arrays::emit_array_to_hash(emitter);
    arrays::emit_array_to_hash_reverse(emitter);
    arrays::emit_array_to_hash_unique(emitter);
    arrays::emit_hash_to_hash_unique(emitter);
    arrays::emit_array_replace(emitter);
    arrays::emit_array_replace_recursive(emitter);
    arrays::emit_assoc_diff_intersect(emitter);
    arrays::emit_amr_box_value(emitter);
    arrays::emit_array_merge_recursive(emitter);
    arrays::emit_array_multisort(emitter);
    arrays::emit_asort(emitter);
    arrays::emit_hash_sort(emitter);
    arrays::emit_natsort(emitter);
    arrays::emit_array_map(emitter);
    arrays::emit_array_map_mixed(emitter);
    arrays::emit_array_map_str(emitter);
    arrays::emit_array_map_str_owned(emitter);
    arrays::emit_array_filter(emitter);
    arrays::emit_array_filter_refcounted(emitter);
    arrays::emit_array_find_any_all(emitter);
    arrays::emit_array_reduce(emitter);
    arrays::emit_array_reduce_str(emitter);
    arrays::emit_array_walk(emitter);
    arrays::emit_array_walk_recursive(emitter);
    arrays::emit_array_udiff_uintersect(emitter);
    arrays::emit_hash_filter(emitter);
    arrays::emit_sort_mixed(emitter);
    arrays::emit_usort(emitter);
    arrays::emit_usort_str(emitter);
    arrays::emit_array_to_mixed(emitter);
    arrays::emit_array_merge_into(emitter);
    arrays::emit_array_merge_into_refcounted(emitter);
    arrays::emit_decref_any(emitter);
    arrays::emit_decref_mixed(emitter);
    arrays::emit_gc_note_child_ref(emitter);
    arrays::emit_gc_mark_reachable(emitter);
    arrays::emit_gc_collect_cycles(emitter);
    arrays::emit_mixed_clone(emitter);
    arrays::emit_mixed_from_value(emitter);
    arrays::emit_mixed_cast_array(emitter);
    arrays::emit_mixed_abs(emitter);
    arrays::emit_mixed_instanceof(emitter);
    arrays::emit_iterable_unsupported_kind(emitter);
    arrays::emit_foreach_non_iterable_warning(emitter);
    arrays::emit_nan_bool_coercion_warning(emitter);
    arrays::emit_iterable_write_stdout(emitter);
    arrays::emit_mixed_cast_bool(emitter);
    arrays::emit_mixed_cast_float(emitter);
    arrays::emit_mixed_cast_int(emitter);
    arrays::emit_mixed_cast_int_nullable(emitter);
    arrays::emit_mixed_intval_base(emitter);
    arrays::emit_mixed_cast_string(emitter);
    arrays::emit_mixed_count(emitter);
    arrays::emit_mixed_free_deep(emitter, features);
    arrays::emit_mixed_is_empty(emitter);
    arrays::emit_mixed_numeric_binops(emitter);
    arrays::emit_int_checked_binops(emitter);
    arrays::emit_int_pow_checked(emitter);
    arrays::emit_mixed_numeric_pow(emitter);
    arrays::emit_mixed_strict_eq(emitter);
    arrays::emit_array_strict_eq(emitter);
    arrays::emit_mixed_unbox(emitter);
    arrays::emit_mixed_write_stdout(emitter);
    arrays::emit_object_free_deep(emitter, features);
    arrays::emit_refcount(emitter);
    if features.eval_bridge {
        eval_bridge::emit_eval_bridge_runtime(emitter);
    } else if features.eval_scope {
        // Scope-only programs run compiled eval fragments natively: they need
        // the self-contained value wrappers plus the native scope helpers
        // (the magician staticlib supplies the scope symbols only in the full
        // bridge configuration).
        eval_bridge::emit_eval_bridge_runtime(emitter);
        eval_scope::emit_eval_scope_runtime(emitter);
    }

    // SPL runtime-managed containers
    spl::emit_doubly_linked_list_runtime(emitter);
    spl::emit_fixed_array_runtime(emitter);

    // PHP resource-id registry (its own numbering space, unrelated to object handles)
    resource_ids::emit_resource_ids(emitter);

    // Object runtime functions
    objects::emit_object_handles(emitter);
    objects::emit_object_to_hash(emitter);
    objects::emit_stdclass_new(emitter);
    objects::emit_stdclass_from_hash(emitter);
    objects::emit_stdclass_get(emitter);
    objects::emit_stdclass_set(emitter);
    objects::emit_mixed_property_get(emitter);
    objects::emit_mixed_property_set(emitter);
    objects::emit_mixed_cell_autovivify_array(emitter);
    objects::emit_mixed_array_get(emitter);
    objects::emit_string_offset_warning(emitter);
    objects::emit_throw_object_not_array(emitter);
    objects::emit_mixed_array_set(emitter);
    objects::emit_mixed_array_append(emitter);
    objects::emit_mixed_array_fetch_for_write(emitter);
    objects::emit_new_by_name(emitter);
    objects::emit_call_object_destructor(emitter);
    // PHP `==` walkers: one boxed-Mixed dispatcher plus the array and object
    // recursion it delegates to. Emitted after the object property accessors it calls.
    compare::emit_mixed_loose_eq(emitter);
    compare::emit_mixed_array_loose_eq(emitter);
    compare::emit_obj_loose_eq(emitter);
    compare::emit_php_compare(emitter);
    arrays::emit_min_max_mixed(emitter);
    arrays::emit_min_max_str(emitter);
    arrays::emit_min_max_hash(emitter);
    objects::emit_obj_enum_kind(emitter);
    objects::emit_obj_enum_name_offset(emitter);
    objects::emit_obj_enum_case_name(emitter);
    objects::emit_var_dump_emit_enum_line(emitter);
    objects::emit_pr_obj_desc(emitter);
    objects::emit_print_r_object(emitter);
    objects::emit_obj_prop_count(emitter);
    objects::emit_obj_prop_name(emitter);
    objects::emit_obj_prop_value(emitter);
    objects::emit_json_encode_stdclass(emitter);

    // Buffer runtime functions
    buffers::emit_buffer_resolve(emitter);
    buffers::emit_buffer_new(emitter);
    buffers::emit_buffer_free(emitter);
    buffers::emit_buffer_len(emitter);
    buffers::emit_buffer_bounds_fail(emitter);
    buffers::emit_buffer_registry_fail(emitter);
    buffers::emit_buffer_use_after_free(emitter);
}
