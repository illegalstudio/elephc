//! Purpose:
//! Declares generated C-ABI runtime wrapper symbols consumed by eval hooks.
//! These declarations are grouped separately so operation code can stay focused
//! on RuntimeValueOps behavior rather than linkage inventory.
//!
//! Called from:
//! - `crate::runtime_hooks::ops` runtime adapter methods.
//! - `crate::runtime_hooks::ElephcRuntimeOps` shared argument packing helpers.
//!
//! Key details:
//! - Symbols are provided by the main elephc runtime object when eval is enabled.
//! - Null return pointers are translated to `EvalStatus::RuntimeFatal` by callers.

use std::ffi::c_void;

use crate::value::{RuntimeCell, RuntimeCellHandle};

#[cfg(not(test))]
unsafe extern "C" {
    pub(super) fn __elephc_eval_value_array_new(capacity: u64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_string_array_new(capacity: u64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_string_array_push(
        array: *mut RuntimeCell,
        value_ptr: *const u8,
        value_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_assoc_new(capacity: u64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_array_get(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_array_key_exists(
        key: *mut RuntimeCell,
        array: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_array_iter_key(
        array: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_array_set(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
        value: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_property_get(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_property_is_initialized(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_value_property_set(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_value_static_property_get(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_static_property_is_initialized(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_value_static_property_set(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_value_class_constant_get(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns a boxed shallow clone for stdClass/eval object storage.
    pub(super) fn __elephc_eval_value_object_clone_shallow(
        object: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns a boxed Mixed object cell for a borrowed raw object payload.
    pub(super) fn __elephc_eval_value_object_from_raw(
        object: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_object_property_len(object: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_value_object_property_iter_key(
        object: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_method_call(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_static_method_call(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_attribute_new(
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        target: u64,
        repeated: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_owner_new(
        owner_kind: u64,
        name_ptr: *const u8,
        name_len: u64,
        attrs: *mut RuntimeCell,
        interface_names: *mut RuntimeCell,
        trait_names: *mut RuntimeCell,
        method_names: *mut RuntimeCell,
        property_names: *mut RuntimeCell,
        method_objects: *mut RuntimeCell,
        property_objects: *mut RuntimeCell,
        parent_class: *mut RuntimeCell,
        flags: u64,
        modifiers: u64,
        method_modifiers: u64,
        constant_value: *mut RuntimeCell,
        backing_value: *mut RuntimeCell,
        constructor: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_method_flags(
        class_ptr: *const u8,
        class_len: u64,
        method_ptr: *const u8,
        method_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_reflection_method_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        method_ptr: *const u8,
        method_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_method_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_source_file() -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_class_flags(
        class_ptr: *const u8,
        class_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_reflection_property_flags(
        class_ptr: *const u8,
        class_len: u64,
        property_ptr: *const u8,
        property_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_reflection_property_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        property_ptr: *const u8,
        property_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_property_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_constant_value(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_constant_flags(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_reflection_constant_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_constant_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_class_interface_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_class_trait_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_class_trait_alias_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_class_trait_alias_sources(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_new_object(
        name_ptr: *const u8,
        name_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_construct_object(
        object: *mut RuntimeCell,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> u64;
    pub(super) fn __elephc_eval_value_take_pending_throwable() -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_class_exists(name_ptr: *const u8, name_len: u64) -> u64;
    pub(super) fn __elephc_eval_interface_exists(name_ptr: *const u8, name_len: u64) -> u64;
    pub(super) fn __elephc_eval_value_is_a(
        object_or_class: *mut RuntimeCell,
        target_ptr: *const u8,
        target_len: u64,
        exclude_self: u64,
    ) -> u64;
    pub(super) fn __elephc_eval_value_object_class_name(
        object: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_parent_class_name(
        object_or_class: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns whether generated trait metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_trait_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Returns whether generated enum metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_enum_exists(name_ptr: *const u8, name_len: u64) -> u64;
    pub(super) fn __elephc_eval_value_array_len(array: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_value_is_array_like(value: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_value_is_null(value: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_value_type_tag(value: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_value_invoker_ref_cell(
        slot: *mut RuntimeCellHandle,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_invoker_raw_ref_cell(
        slot: *mut c_void,
        source_tag: u64,
    ) -> *mut RuntimeCell;
    /// Extracts the low raw payload word from a boxed runtime value.
    pub(super) fn __elephc_eval_value_raw_word(value: *mut RuntimeCell) -> u64;
    /// Extracts the high raw payload word from a boxed runtime value.
    pub(super) fn __elephc_eval_value_raw_high_word(value: *mut RuntimeCell) -> u64;
    /// Duplicates raw string storage for a staged native by-reference slot.
    pub(super) fn __elephc_eval_value_retain_raw_string(
        ptr: u64,
        len: u64,
        out_len: *mut u64,
    ) -> u64;
    /// Boxes raw string storage back into a runtime value for eval writeback.
    pub(super) fn __elephc_eval_value_from_raw_string(ptr: u64, len: u64) -> *mut RuntimeCell;
    /// Releases raw string storage owned by a staged native by-reference slot.
    pub(super) fn __elephc_eval_value_release_raw_string(ptr: u64, len: u64);
    /// Retains one raw heap payload word for a staged native by-reference slot.
    pub(super) fn __elephc_eval_value_retain_raw_heap_word(word: u64) -> u64;
    /// Boxes one one-word raw payload back into a runtime value using a known tag.
    pub(super) fn __elephc_eval_value_from_raw_word(
        source_tag: u64,
        word: u64,
    ) -> *mut RuntimeCell;
    /// Boxes one raw heap payload word back into a runtime value.
    pub(super) fn __elephc_eval_value_from_raw_heap_word(word: u64) -> *mut RuntimeCell;
    /// Releases one raw heap payload word owned by a staged by-reference slot.
    pub(super) fn __elephc_eval_value_release_raw_heap_word(word: u64);
    /// Returns the unboxed object payload pointer for object-tagged eval values.
    pub(super) fn __elephc_eval_value_object_identity(value: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_warning(message_ptr: *const u8, message_len: u64);
    pub(super) fn __elephc_eval_value_null() -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_bool(value: u64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_int(value: i64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_resource(value: i64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_float(value: f64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_string(ptr: *const u8, len: u64) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_cast_int(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_cast_float(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_cast_string(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_cast_bool(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_abs(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_ceil(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_floor(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_sqrt(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_strrev(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_fdiv(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_fmod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_add(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_sub(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_mul(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_div(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_mod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_pow(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_round(
        value: *mut RuntimeCell,
        precision: *mut RuntimeCell,
        has_precision: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_bitwise(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_bit_not(value: *mut RuntimeCell) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_concat(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_spaceship(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_echo(value: *mut RuntimeCell);
    pub(super) fn __elephc_eval_value_string_bytes(
        value: *mut RuntimeCell,
        out_ptr: *mut *const u8,
        out_len: *mut u64,
    ) -> u64;
    pub(super) fn __elephc_eval_value_truthy(value: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_ob_start() -> i64;
    pub(super) fn __elephc_eval_ob_level() -> i64;
    pub(super) fn __elephc_eval_ob_length() -> i64;
    pub(super) fn __elephc_eval_ob_clean() -> i64;
    pub(super) fn __elephc_eval_ob_flush() -> i64;
    pub(super) fn __elephc_eval_ob_end(flush: i64) -> i64;
    pub(super) fn __elephc_eval_ob_contents(out_ptr: *mut *const u8, out_len: *mut i64) -> i64;
    pub(super) fn __elephc_eval_ob_stats(index: i64, out_used: *mut i64, out_size: *mut i64)
        -> i64;
    pub(super) fn __elephc_eval_ob_implicit_flush(enable: i64);
    pub(super) fn __elephc_eval_ob_start_ex(
        has_handler: i64,
        handler_id: i64,
        chunk_size: i64,
        flags: i64,
        name_ptr: *const u8,
        name_len: i64,
    ) -> i64;
    pub(super) fn __elephc_eval_ob_get_clean_pop(out_ptr: *mut *const u8, out_len: *mut i64)
        -> i64;
    pub(super) fn __elephc_eval_ob_get_flush_pop(out_ptr: *mut *const u8, out_len: *mut i64)
        -> i64;
    pub(super) fn __elephc_eval_ob_release_string(ptr: *const u8);
    pub(super) fn __elephc_eval_ob_slot_meta(
        index: i64,
        out_chunk: *mut i64,
        out_flags: *mut i64,
        out_user_started: *mut i64,
    ) -> i64;
    pub(super) fn __elephc_eval_ob_slot_name(
        index: i64,
        out_ptr: *mut *const u8,
        out_len: *mut i64,
    ) -> i64;
    pub(super) fn __elephc_eval_install_ob_handler_hook(callback: usize);
    pub(super) fn __elephc_eval_value_final_object_identity(value: *mut RuntimeCell) -> u64;
    pub(super) fn __elephc_eval_value_release(value: *mut RuntimeCell);
    pub(super) fn __elephc_eval_value_retain(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Installs the optional eval dynamic object destructor callback.
    pub(super) fn __elephc_eval_install_dynamic_object_destructor_hook(callback: usize);
}

/// Forwards one installed eval ob-handler callback address to the generated runtime.
///
/// # Safety
/// `callback` must follow the eval ob-handler ABI; see
/// `crate::runtime_hooks::install_ob_handler_hook`.
pub(super) unsafe fn install_ob_handler_hook_raw(callback: usize) {
    unsafe {
        __elephc_eval_install_ob_handler_hook(callback);
    }
}
