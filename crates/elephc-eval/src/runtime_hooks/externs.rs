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

use crate::value::RuntimeCell;

#[cfg(not(test))]
unsafe extern "C" {
    pub(super) fn __elephc_eval_value_array_new(capacity: u64) -> *mut RuntimeCell;
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
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_property_set(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
    ) -> u64;
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
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_static_method_call(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_attribute_new(
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_reflection_owner_new(
        owner_kind: u64,
        name_ptr: *const u8,
        name_len: u64,
        attrs: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_new_object(
        name_ptr: *const u8,
        name_len: u64,
    ) -> *mut RuntimeCell;
    pub(super) fn __elephc_eval_value_construct_object(
        object: *mut RuntimeCell,
        args: *mut RuntimeCell,
    ) -> u64;
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
    pub(super) fn __elephc_eval_value_release(value: *mut RuntimeCell);
    pub(super) fn __elephc_eval_value_retain(value: *mut RuntimeCell) -> *mut RuntimeCell;
}
