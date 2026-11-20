//! Purpose:
//! Collects value storage, COW and deep-retirement assembly emitters.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub mod array_new;
pub mod array_grow;
pub mod hash_new;
pub mod hash_grow;
pub mod hash_pin;
pub mod mixed_reuse_int_cell;
pub mod array_free_deep;
pub mod hash_free_deep;
pub mod object_free_deep;
pub mod mixed_free_deep;
pub mod array_ensure_unique;
pub mod hash_ensure_unique;
pub mod array_clone_shallow;
pub mod hash_clone_shallow;
pub mod mixed_clone;
pub mod deep_cleanup;
pub mod native_reference_source_retirement;
pub mod hash_write_guard;
pub mod mixed_from_value;
pub mod resource_weak;
pub mod mixed_reference;

pub mod mixed_unbox;

pub mod array_unbox_slots;

pub mod owned_container;

pub mod owned_payload;

pub mod boxed_array;
pub mod boxed_offset_row;
pub mod array_to_mixed;
pub mod array_to_pointer_words;
pub mod array_header;
pub mod hash_to_mixed;
pub mod array_to_hash;
pub mod array_set_refcounted;
pub mod hash_unset;
pub mod object_clone_slots;
pub mod eval_scope_storage;
pub mod reference_registry;
pub mod reference_snapshot;
pub mod stack_registry_snapshot;

pub mod array_set_int;
pub mod array_set_str;
pub mod array_set_mixed;
pub mod array_push_int;
pub mod array_push_str;
pub mod array_push_refcounted;
pub mod array_merge_into;
pub mod array_merge_into_refcounted;
pub mod array_shift;
pub mod array_splice_insert;
pub mod array_splice_remove;
pub mod array_unshift;
pub mod hash_iter;
pub mod hash_bucket;
pub mod hash_payload;
pub mod hash_fnv1a;
pub mod hash_get;
pub mod hash_key_eq;
pub mod hash_key_hash;
pub mod hash_next_index;
pub mod hash_insert_owned;
pub mod hash_reindex_dense;
pub mod object_storage;
pub mod throwable_storage;
pub mod incomplete_object;

pub mod throwable_previous;

pub mod fixed_array_storage;
pub mod packed_mixed_copy;
pub mod packed_mixed_header;
pub mod packed_mixed_prefix;
pub mod packed_slice;
pub mod destructor_header;
pub mod spl_storage;
pub mod stack_cell;
