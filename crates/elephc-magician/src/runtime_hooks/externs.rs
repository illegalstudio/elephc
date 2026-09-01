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
    /// Calls one typed generated-runtime builtin over borrowed boxed arguments.
    pub(super) fn __elephc_runtime_builtin_call_v1(
        runtime_builtin_id: u32,
        args: *const *mut RuntimeCell,
        arg_count: u64,
        context: *const c_void,
        result_out: *mut *mut RuntimeCell,
    ) -> i32;
    /// Allocates an eval runtime array with the requested initial capacity.
    pub(super) fn __elephc_eval_value_array_new(capacity: u64) -> *mut RuntimeCell;
    /// Allocates an eval runtime array specialized for string entries.
    pub(super) fn __elephc_eval_value_string_array_new(capacity: u64) -> *mut RuntimeCell;
    /// Appends one borrowed byte string to an eval string array.
    pub(super) fn __elephc_eval_value_string_array_push(
        array: *mut RuntimeCell,
        value_ptr: *const u8,
        value_len: u64,
    ) -> *mut RuntimeCell;
    /// Allocates an eval runtime associative array with the requested capacity.
    pub(super) fn __elephc_eval_value_assoc_new(capacity: u64) -> *mut RuntimeCell;
    /// Returns the boxed value stored under an eval array index.
    pub(super) fn __elephc_eval_value_array_get(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Reports whether an eval array contains the supplied key.
    pub(super) fn __elephc_eval_value_array_key_exists(
        key: *mut RuntimeCell,
        array: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed key at an eval array iteration position.
    pub(super) fn __elephc_eval_value_array_iter_key(
        array: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    /// Stores a boxed value under an eval array index and returns the array cell.
    pub(super) fn __elephc_eval_value_array_set(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
        value: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Reads an object property using PHP visibility rules for the supplied scope.
    pub(super) fn __elephc_eval_value_property_get(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    /// Reports whether an object property is initialized in the supplied scope.
    pub(super) fn __elephc_eval_value_property_is_initialized(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Writes an object property using PHP visibility rules for the supplied scope.
    pub(super) fn __elephc_eval_value_property_set(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Reads a static property using PHP visibility rules for the supplied scope.
    pub(super) fn __elephc_eval_value_static_property_get(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    /// Reports whether a static property is initialized in the supplied scope.
    pub(super) fn __elephc_eval_value_static_property_is_initialized(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Writes a static property using PHP visibility rules for the supplied scope.
    pub(super) fn __elephc_eval_value_static_property_set(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Reads a class constant using PHP visibility rules for the supplied scope.
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
    /// Returns the number of visible properties stored on an eval object.
    pub(super) fn __elephc_eval_value_object_property_len(object: *mut RuntimeCell) -> u64;
    /// Returns the boxed property name at an eval object iteration position.
    pub(super) fn __elephc_eval_value_object_property_iter_key(
        object: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    /// Invokes an instance method with boxed arguments and the active eval context.
    pub(super) fn __elephc_eval_value_method_call(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> *mut RuntimeCell;
    /// Invokes a static method with boxed arguments and the active eval context.
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
    /// Constructs a boxed reflection attribute from its metadata and arguments.
    pub(super) fn __elephc_eval_reflection_attribute_new(
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        target: u64,
        repeated: u64,
    ) -> *mut RuntimeCell;
    /// Constructs a boxed reflection owner from generated declaration metadata.
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
    /// Returns generated visibility and declaration flags for one method.
    pub(super) fn __elephc_eval_reflection_method_flags(
        class_ptr: *const u8,
        class_len: u64,
        method_ptr: *const u8,
        method_len: u64,
    ) -> u64;
    /// Returns the boxed declaring-class name for one reflected method.
    pub(super) fn __elephc_eval_reflection_method_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        method_ptr: *const u8,
        method_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the generated method-name array for a reflected class.
    pub(super) fn __elephc_eval_reflection_method_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the boxed source filename associated with reflection metadata.
    pub(super) fn __elephc_eval_reflection_source_file() -> *mut RuntimeCell;
    /// Returns generated declaration flags for a reflected class.
    pub(super) fn __elephc_eval_reflection_class_flags(
        class_ptr: *const u8,
        class_len: u64,
    ) -> u64;
    /// Returns generated visibility and declaration flags for one property.
    pub(super) fn __elephc_eval_reflection_property_flags(
        class_ptr: *const u8,
        class_len: u64,
        property_ptr: *const u8,
        property_len: u64,
    ) -> u64;
    /// Returns the boxed declaring-class name for one reflected property.
    pub(super) fn __elephc_eval_reflection_property_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        property_ptr: *const u8,
        property_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the generated property-name array for a reflected class.
    pub(super) fn __elephc_eval_reflection_property_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the boxed value of one generated class constant.
    pub(super) fn __elephc_eval_reflection_constant_value(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns generated visibility and declaration flags for one class constant.
    pub(super) fn __elephc_eval_reflection_constant_flags(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> u64;
    /// Returns the boxed declaring-class name for one reflected class constant.
    pub(super) fn __elephc_eval_reflection_constant_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the generated class-constant name array for a reflected class.
    pub(super) fn __elephc_eval_reflection_constant_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the generated interface-name array for a reflected class.
    pub(super) fn __elephc_eval_reflection_class_interface_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the generated trait-name array for a reflected class.
    pub(super) fn __elephc_eval_reflection_class_trait_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the generated trait-alias name array for a reflected class.
    pub(super) fn __elephc_eval_reflection_class_trait_alias_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns source-method metadata for the generated trait aliases of a class.
    pub(super) fn __elephc_eval_reflection_class_trait_alias_sources(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Allocates an unconstructed boxed object for the requested class name.
    pub(super) fn __elephc_eval_value_new_object(
        name_ptr: *const u8,
        name_len: u64,
    ) -> *mut RuntimeCell;
    /// Runs the constructor of a boxed object with eval arguments and scope context.
    pub(super) fn __elephc_eval_value_construct_object(
        object: *mut RuntimeCell,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> u64;
    /// Takes and clears the throwable pending after an eval runtime call.
    pub(super) fn __elephc_eval_value_take_pending_throwable() -> *mut RuntimeCell;
    /// Reports whether generated class metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_class_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Reports whether generated interface metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_interface_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Applies PHP `is_a` semantics to a boxed object or class-name value.
    pub(super) fn __elephc_eval_value_is_a(
        object_or_class: *mut RuntimeCell,
        target_ptr: *const u8,
        target_len: u64,
        exclude_self: u64,
    ) -> u64;
    /// Returns the boxed runtime class name of an object value.
    pub(super) fn __elephc_eval_value_object_class_name(
        object: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed parent-class name of an object or class-name value.
    pub(super) fn __elephc_eval_value_parent_class_name(
        object_or_class: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns whether generated trait metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_trait_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Returns whether generated enum metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_enum_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Returns the logical entry count of a boxed eval array.
    pub(super) fn __elephc_eval_value_array_len(array: *mut RuntimeCell) -> u64;
    /// Reports whether a boxed value uses an array-compatible runtime shape.
    pub(super) fn __elephc_eval_value_is_array_like(value: *mut RuntimeCell) -> u64;
    /// Reports whether a boxed value carries PHP's null tag.
    pub(super) fn __elephc_eval_value_is_null(value: *mut RuntimeCell) -> u64;
    /// Returns the runtime tag stored in a boxed eval value.
    pub(super) fn __elephc_eval_value_type_tag(value: *mut RuntimeCell) -> u64;
    /// Returns the boxed cell referenced by an eval invoker handle slot.
    pub(super) fn __elephc_eval_value_invoker_ref_cell(
        slot: *mut RuntimeCellHandle,
    ) -> *mut RuntimeCell;
    /// Boxes a native by-reference slot using its known runtime source tag.
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
    /// Returns the PHP object handle (`spl_object_id`) for object-tagged eval values.
    pub(super) fn __elephc_eval_value_object_handle(value: *mut RuntimeCell) -> u64;
    /// Emits one eval warning from a borrowed UTF-8 message buffer.
    pub(super) fn __elephc_eval_warning(message_ptr: *const u8, message_len: u64);
    /// Emits one eval deprecation from a borrowed UTF-8 message buffer.
    pub(super) fn __elephc_eval_deprecated(message_ptr: *const u8, message_len: u64);
    /// Gets or updates the active eval error-reporting mask.
    pub(super) fn __elephc_eval_error_reporting(level: i64, has_level: u64) -> i64;
    /// Allocates a boxed PHP null value.
    pub(super) fn __elephc_eval_value_null() -> *mut RuntimeCell;
    /// Allocates a boxed PHP boolean value from a zero-or-one word.
    pub(super) fn __elephc_eval_value_bool(value: u64) -> *mut RuntimeCell;
    /// Allocates a boxed PHP integer value.
    pub(super) fn __elephc_eval_value_int(value: i64) -> *mut RuntimeCell;
    /// Allocates a boxed PHP resource identifier.
    pub(super) fn __elephc_eval_value_resource(value: i64) -> *mut RuntimeCell;
    /// Boxes an eval hash-context table key as an inert (id-less, destructor-less) resource.
    pub(super) fn __elephc_eval_value_hash_context(value: i64) -> *mut RuntimeCell;
    /// Allocates a boxed PHP floating-point value.
    pub(super) fn __elephc_eval_value_float(value: f64) -> *mut RuntimeCell;
    /// Allocates a boxed PHP string by copying the supplied bytes.
    pub(super) fn __elephc_eval_value_string(ptr: *const u8, len: u64) -> *mut RuntimeCell;
    /// Applies PHP integer-cast semantics to a boxed value.
    pub(super) fn __elephc_eval_value_cast_int(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Applies PHP float-cast semantics to a boxed value.
    pub(super) fn __elephc_eval_value_cast_float(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Applies PHP string-cast semantics to a boxed value.
    pub(super) fn __elephc_eval_value_cast_string(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Applies PHP boolean-cast semantics to a boxed value.
    pub(super) fn __elephc_eval_value_cast_bool(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed PHP absolute value of a numeric operand.
    pub(super) fn __elephc_eval_value_abs(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed ceiling of a numeric operand.
    pub(super) fn __elephc_eval_value_ceil(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed floor of a numeric operand.
    pub(super) fn __elephc_eval_value_floor(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed square root of a numeric operand.
    pub(super) fn __elephc_eval_value_sqrt(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns a boxed string with the operand bytes reversed.
    pub(super) fn __elephc_eval_value_strrev(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Applies PHP floating-point division to two boxed operands.
    pub(super) fn __elephc_eval_value_fdiv(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP floating-point remainder semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_fmod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP addition semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_add(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP subtraction semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_sub(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP multiplication semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_mul(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP division semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_div(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP modulo semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_mod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies PHP exponentiation semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_pow(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Rounds a boxed numeric value with an optional boxed precision.
    pub(super) fn __elephc_eval_value_round(
        value: *mut RuntimeCell,
        precision: *mut RuntimeCell,
        has_precision: u64,
    ) -> *mut RuntimeCell;
    /// Applies the selected PHP bitwise operation to two boxed operands.
    pub(super) fn __elephc_eval_value_bitwise(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    /// Applies PHP bitwise-complement semantics to a boxed operand.
    pub(super) fn __elephc_eval_value_bit_not(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Concatenates two boxed operands using PHP string conversion rules.
    pub(super) fn __elephc_eval_value_concat(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Applies the selected PHP comparison operation to two boxed operands.
    pub(super) fn __elephc_eval_value_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    /// Applies PHP three-way comparison semantics to two boxed operands.
    pub(super) fn __elephc_eval_value_spaceship(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Compares normalized array keys with the compiled runtime's regular ordering.
    pub(super) fn __elephc_eval_value_regular_key_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> i64;
    /// Writes a boxed value using PHP `echo` conversion semantics.
    pub(super) fn __elephc_eval_value_echo(value: *mut RuntimeCell);
    /// Borrows the byte pointer and length from a boxed PHP string value.
    pub(super) fn __elephc_eval_value_string_bytes(
        value: *mut RuntimeCell,
        out_ptr: *mut *const u8,
        out_len: *mut u64,
    ) -> u64;
    /// Returns PHP truthiness for a boxed runtime value.
    pub(super) fn __elephc_eval_value_truthy(value: *mut RuntimeCell) -> u64;
    /// Starts one default eval output-buffer frame.
    pub(super) fn __elephc_eval_ob_start() -> i64;
    /// Returns the active eval output-buffer nesting level.
    pub(super) fn __elephc_eval_ob_level() -> i64;
    /// Returns the byte length of the active eval output buffer.
    pub(super) fn __elephc_eval_ob_length() -> i64;
    /// Clears the active eval output buffer without removing it.
    pub(super) fn __elephc_eval_ob_clean() -> i64;
    /// Flushes the active eval output buffer without removing it.
    pub(super) fn __elephc_eval_ob_flush() -> i64;
    /// Ends the active eval output buffer, optionally flushing its contents.
    pub(super) fn __elephc_eval_ob_end(flush: i64) -> i64;
    /// Borrows the bytes currently stored in the active eval output buffer.
    pub(super) fn __elephc_eval_ob_contents(out_ptr: *mut *const u8, out_len: *mut i64) -> i64;
    /// Returns usage statistics for one eval output-buffer slot.
    pub(super) fn __elephc_eval_ob_stats(index: i64, out_used: *mut i64, out_size: *mut i64)
        -> i64;
    /// Enables or disables implicit flushing for eval output buffers.
    pub(super) fn __elephc_eval_ob_implicit_flush(enable: i64);
    /// Starts a configured eval output buffer with handler metadata.
    pub(super) fn __elephc_eval_ob_start_ex(
        has_handler: i64,
        handler_id: i64,
        chunk_size: i64,
        flags: i64,
        name_ptr: *const u8,
        name_len: i64,
    ) -> i64;
    /// Removes the active eval output buffer and returns its unflushed bytes.
    pub(super) fn __elephc_eval_ob_get_clean_pop(out_ptr: *mut *const u8, out_len: *mut i64)
        -> i64;
    /// Flushes and removes the active eval output buffer, returning its bytes.
    pub(super) fn __elephc_eval_ob_get_flush_pop(out_ptr: *mut *const u8, out_len: *mut i64)
        -> i64;
    /// Releases a string allocation returned by an eval output-buffer operation.
    pub(super) fn __elephc_eval_ob_release_string(ptr: *const u8);
    /// Returns configuration metadata for one eval output-buffer slot.
    pub(super) fn __elephc_eval_ob_slot_meta(
        index: i64,
        out_chunk: *mut i64,
        out_flags: *mut i64,
        out_user_started: *mut i64,
    ) -> i64;
    /// Returns the configured handler name for one eval output-buffer slot.
    pub(super) fn __elephc_eval_ob_slot_name(
        index: i64,
        out_ptr: *mut *const u8,
        out_len: *mut i64,
    ) -> i64;
    /// Installs the generated-runtime callback used for eval output handlers.
    pub(super) fn __elephc_eval_install_ob_handler_hook(callback: usize);
    /// Returns the final raw object identity associated with a boxed eval value.
    pub(super) fn __elephc_eval_value_final_object_identity(value: *mut RuntimeCell) -> u64;
    /// Releases one owned reference to a boxed eval value.
    pub(super) fn __elephc_eval_value_release(value: *mut RuntimeCell);
    /// Retains and returns one boxed eval value.
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
