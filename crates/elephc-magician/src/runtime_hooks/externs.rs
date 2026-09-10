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
    /// Allocates a persistent reference containing a copied PHP value.
    pub(super) fn __elephc_eval_value_reference_new(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Classifies a raw boxed value without dereferencing its current PHP value.
    pub(super) fn __elephc_eval_value_is_reference(value: *mut RuntimeCell) -> u64;
    /// Replaces a reference's value and transfers the previous owned cell to the caller.
    pub(super) fn __elephc_eval_value_reference_replace(reference: *mut RuntimeCell, value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns a detached PHP value copy, preserving object and resource value identity.
    pub(super) fn __elephc_eval_value_copy(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Calls one typed generated-runtime builtin over borrowed boxed arguments.
    pub(super) fn __elephc_runtime_builtin_call_v1(
        runtime_builtin_id: u32,
        args: *const *mut RuntimeCell,
        arg_count: u64,
        context: *const c_void,
        result_out: *mut *mut RuntimeCell,
    ) -> i32;
    /// Allocates a boxed indexed array with the requested initial capacity.
    pub(super) fn __elephc_eval_value_array_new(capacity: u64) -> *mut RuntimeCell;
    /// Allocates boxed array storage specialized for string elements.
    pub(super) fn __elephc_eval_value_string_array_new(capacity: u64) -> *mut RuntimeCell;
    /// Appends the supplied byte string to a boxed string array.
    pub(super) fn __elephc_eval_value_string_array_push(
        array: *mut RuntimeCell,
        value_ptr: *const u8,
        value_len: u64,
    ) -> *mut RuntimeCell;
    /// Allocates a boxed associative array with the requested initial capacity.
    pub(super) fn __elephc_eval_value_assoc_new(capacity: u64) -> *mut RuntimeCell;
    /// Reads a boxed array element after normalizing the supplied PHP key.
    pub(super) fn __elephc_eval_value_array_get(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Boxes whether the supplied key exists in the array.
    pub(super) fn __elephc_eval_value_array_key_exists(
        key: *mut RuntimeCell,
        array: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed key at the requested insertion-order position.
    pub(super) fn __elephc_eval_value_array_iter_key(
        array: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    /// Copies one element by exact insertion-order position without normalizing keys.
    pub(super) fn __elephc_eval_value_array_iter_value(array: *mut RuntimeCell, position: u64) -> *mut RuntimeCell;
    /// Stores a copied element under the normalized key and returns the mutated array box.
    pub(super) fn __elephc_eval_value_array_set(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
        value: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Reads an instance property using the supplied visibility scope.
    pub(super) fn __elephc_eval_value_property_get(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    /// Tests whether an instance property is initialized and visible in the supplied scope.
    pub(super) fn __elephc_eval_value_property_is_initialized(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Writes an instance property using the supplied visibility scope.
    pub(super) fn __elephc_eval_value_property_set(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Reads a static property using the named class and visibility scope.
    pub(super) fn __elephc_eval_value_static_property_get(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> *mut RuntimeCell;
    /// Tests initialization of a scoped static property.
    pub(super) fn __elephc_eval_value_static_property_is_initialized(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Writes a static property using the named class and visibility scope.
    pub(super) fn __elephc_eval_value_static_property_set(
        class_ptr: *const u8,
        class_len: u64,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
    ) -> u64;
    /// Returns a class constant through the supplied visibility scope.
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
    /// Returns the number of enumerable properties in the boxed object.
    pub(super) fn __elephc_eval_value_object_property_len(object: *mut RuntimeCell) -> u64;
    /// Returns the boxed property name at an enumeration position.
    pub(super) fn __elephc_eval_value_object_property_iter_key(
        object: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    /// Invokes a scoped native instance method over a borrowed argument array.
    pub(super) fn __elephc_eval_value_method_call(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> *mut RuntimeCell;
    /// Invokes a scoped native static method over a borrowed argument array.
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
    /// Constructs a boxed ReflectionAttribute from its name, arguments, and target flags.
    pub(super) fn __elephc_eval_reflection_attribute_new(
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
        target: u64,
        repeated: u64,
    ) -> *mut RuntimeCell;
    /// Constructs a boxed reflection owner from prepared member and hierarchy metadata.
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
    /// Returns generated flags for the named native method.
    pub(super) fn __elephc_eval_reflection_method_flags(
        class_ptr: *const u8,
        class_len: u64,
        method_ptr: *const u8,
        method_len: u64,
    ) -> u64;
    /// Returns the boxed declaring-class name for a native method.
    pub(super) fn __elephc_eval_reflection_method_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        method_ptr: *const u8,
        method_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed method names from the generated class metadata.
    pub(super) fn __elephc_eval_reflection_method_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the boxed source-file path recorded by generated reflection metadata.
    pub(super) fn __elephc_eval_reflection_source_file() -> *mut RuntimeCell;
    /// Returns flags from the generated metadata for a named native class.
    pub(super) fn __elephc_eval_reflection_class_flags(
        class_ptr: *const u8,
        class_len: u64,
    ) -> u64;
    /// Returns generated flags for the named native property.
    pub(super) fn __elephc_eval_reflection_property_flags(
        class_ptr: *const u8,
        class_len: u64,
        property_ptr: *const u8,
        property_len: u64,
    ) -> u64;
    /// Returns the boxed declaring-class name for a native property.
    pub(super) fn __elephc_eval_reflection_property_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        property_ptr: *const u8,
        property_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed property names from the generated class metadata.
    pub(super) fn __elephc_eval_reflection_property_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns the boxed value of a reflected native class constant.
    pub(super) fn __elephc_eval_reflection_constant_value(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns generated flags for the named native constant.
    pub(super) fn __elephc_eval_reflection_constant_flags(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> u64;
    /// Returns the boxed declaring-class name for a native constant.
    pub(super) fn __elephc_eval_reflection_constant_declaring_class(
        class_ptr: *const u8,
        class_len: u64,
        constant_ptr: *const u8,
        constant_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed constant names from the generated class metadata.
    pub(super) fn __elephc_eval_reflection_constant_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed interface names from native class metadata.
    pub(super) fn __elephc_eval_reflection_class_interface_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed trait names from native class metadata.
    pub(super) fn __elephc_eval_reflection_class_trait_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed names of aliases introduced by native trait composition.
    pub(super) fn __elephc_eval_reflection_class_trait_alias_names(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Returns boxed source methods corresponding to native trait aliases.
    pub(super) fn __elephc_eval_reflection_class_trait_alias_sources(
        class_ptr: *const u8,
        class_len: u64,
    ) -> *mut RuntimeCell;
    /// Allocates a boxed instance of the named native class before constructor dispatch.
    pub(super) fn __elephc_eval_value_new_object(
        name_ptr: *const u8,
        name_len: u64,
    ) -> *mut RuntimeCell;
    /// Runs the native constructor with prepared arguments and an explicit calling scope.
    pub(super) fn __elephc_eval_value_construct_object(
        object: *mut RuntimeCell,
        args: *mut RuntimeCell,
        scope_ptr: *const u8,
        scope_len: u64,
        context: *const c_void,
    ) -> u64;
    /// Transfers the pending native Throwable into a boxed eval value.
    pub(super) fn __elephc_eval_value_take_pending_throwable() -> *mut RuntimeCell;
    /// Consumes an owned Mixed value and returns a contained native exception status.
    pub(super) fn __elephc_eval_value_release_protected(value: *mut RuntimeCell) -> u64;
    /// Completes a native cycle scan and returns a contained exception status.
    pub(super) fn __elephc_eval_collect_cycles() -> u64;
    /// Tests whether generated native class metadata contains the given name.
    pub(super) fn __elephc_eval_class_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Tests whether generated native interface metadata contains the given name.
    pub(super) fn __elephc_eval_interface_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Tests the object or class hierarchy, optionally excluding the class itself.
    pub(super) fn __elephc_eval_value_is_a(
        object_or_class: *mut RuntimeCell,
        target_ptr: *const u8,
        target_len: u64,
        exclude_self: u64,
    ) -> u64;
    /// Returns the class name associated with a boxed object.
    pub(super) fn __elephc_eval_value_object_class_name(
        object: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the parent class name for a boxed object or class-name value.
    pub(super) fn __elephc_eval_value_parent_class_name(
        object_or_class: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns whether generated trait metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_trait_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Returns whether generated enum metadata contains the requested PHP name.
    pub(super) fn __elephc_eval_enum_exists(name_ptr: *const u8, name_len: u64) -> u64;
    /// Returns the element count of a boxed array.
    pub(super) fn __elephc_eval_value_array_len(array: *mut RuntimeCell) -> u64;
    /// Tests whether the boxed value uses a supported array representation.
    pub(super) fn __elephc_eval_value_is_array_like(value: *mut RuntimeCell) -> u64;
    /// Tests whether the current dereferenced PHP value is null.
    pub(super) fn __elephc_eval_value_is_null(value: *mut RuntimeCell) -> u64;
    /// Returns the runtime tag of the current dereferenced PHP value.
    pub(super) fn __elephc_eval_value_type_tag(value: *mut RuntimeCell) -> u64;
    /// Wraps a caller-owned boxed pointer slot for native callable reference binding.
    pub(super) fn __elephc_eval_value_invoker_ref_cell(
        slot: *mut RuntimeCellHandle,
    ) -> *mut RuntimeCell;
    /// Wraps a typed raw slot for native callable reference binding.
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
    /// Emits the supplied warning bytes through the native diagnostic path.
    pub(super) fn __elephc_eval_warning(message_ptr: *const u8, message_len: u64);
    /// Emits a fatal diagnostic and terminates through the native runtime path.
    pub(super) fn __elephc_eval_fatal(message_ptr: *const u8, message_len: u64);
    /// Mirrors Magician's signal-dispatch region into the generated runtime Fiber guard.
    pub(super) fn __elephc_eval_set_pcntl_dispatching(active: u64);
    /// Allocates a boxed PHP null value.
    pub(super) fn __elephc_eval_value_null() -> *mut RuntimeCell;
    /// Allocates a boxed PHP boolean from the supplied truth value.
    pub(super) fn __elephc_eval_value_bool(value: u64) -> *mut RuntimeCell;
    /// Allocates a boxed PHP signed integer.
    pub(super) fn __elephc_eval_value_int(value: i64) -> *mut RuntimeCell;
    /// Boxes a native resource payload for eval operations.
    pub(super) fn __elephc_eval_value_resource(value: i64) -> *mut RuntimeCell;
    /// Boxes an eval hash-context table key as an inert (id-less, destructor-less) resource.
    pub(super) fn __elephc_eval_value_hash_context(value: i64) -> *mut RuntimeCell;
    /// Allocates a boxed PHP floating-point value.
    pub(super) fn __elephc_eval_value_float(value: f64) -> *mut RuntimeCell;
    /// Copies and boxes a newly produced string using ordinary native ownership.
    pub(super) fn __elephc_eval_value_string(ptr: *const u8, len: u64) -> *mut RuntimeCell;
    /// Copies a PHP literal and gives the boxed native payload an interned logical origin.
    pub(super) fn __elephc_eval_value_string_literal(ptr: *const u8, len: u64) -> *mut RuntimeCell;
    /// Converts the input into a boxed PHP integer value.
    pub(super) fn __elephc_eval_value_cast_int(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Converts the input into a boxed PHP floating-point value.
    pub(super) fn __elephc_eval_value_cast_float(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Converts the input into a boxed PHP string value.
    pub(super) fn __elephc_eval_value_cast_string(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Converts the input into a boxed PHP boolean value.
    pub(super) fn __elephc_eval_value_cast_bool(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed absolute value of a numeric input.
    pub(super) fn __elephc_eval_value_abs(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed floating-point value rounded toward positive infinity.
    pub(super) fn __elephc_eval_value_ceil(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed floating-point value rounded toward negative infinity.
    pub(super) fn __elephc_eval_value_floor(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed square root of a numeric input.
    pub(super) fn __elephc_eval_value_sqrt(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed bytewise reversal of the input string.
    pub(super) fn __elephc_eval_value_strrev(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns a boxed floating-point quotient with PHP fdiv semantics.
    pub(super) fn __elephc_eval_value_fdiv(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed floating-point remainder of the supplied operands.
    pub(super) fn __elephc_eval_value_fmod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed result of PHP addition for the supplied operands.
    pub(super) fn __elephc_eval_value_add(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed result of PHP subtraction for the supplied operands.
    pub(super) fn __elephc_eval_value_sub(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed result of PHP multiplication for the supplied operands.
    pub(super) fn __elephc_eval_value_mul(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed result of PHP division for the supplied operands.
    pub(super) fn __elephc_eval_value_div(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed integer remainder of the supplied operands.
    pub(super) fn __elephc_eval_value_mod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Returns the boxed result of raising the left operand to the right operand.
    pub(super) fn __elephc_eval_value_pow(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Rounds a numeric value using the optional boxed precision argument.
    pub(super) fn __elephc_eval_value_round(
        value: *mut RuntimeCell,
        precision: *mut RuntimeCell,
        has_precision: u64,
    ) -> *mut RuntimeCell;
    /// Applies the selected binary bitwise operation to boxed operands.
    pub(super) fn __elephc_eval_value_bitwise(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    /// Returns the boxed bitwise complement of the input value.
    pub(super) fn __elephc_eval_value_bit_not(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Returns the boxed concatenation of the supplied PHP values.
    pub(super) fn __elephc_eval_value_concat(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Boxes the result of the selected PHP comparison operation.
    pub(super) fn __elephc_eval_value_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    /// Boxes the three-way PHP comparison result for the supplied operands.
    pub(super) fn __elephc_eval_value_spaceship(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    /// Compares normalized array keys with the compiled runtime's regular ordering.
    pub(super) fn __elephc_eval_value_regular_key_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> i64;
    /// Runs a borrowed output request without unwinding through the Rust caller.
    pub(super) fn __elephc_eval_output_v1(
        request: *mut elephc_builtin_contract::output_abi::OutputRequestV1,
    ) -> u64;
    /// Exposes a boxed string through caller-provided pointer and length outputs.
    pub(super) fn __elephc_eval_value_string_bytes(
        value: *mut RuntimeCell,
        out_ptr: *mut *const u8,
        out_len: *mut u64,
    ) -> u64;
    /// Returns the PHP truth value of a boxed runtime value.
    pub(super) fn __elephc_eval_value_truthy(value: *mut RuntimeCell) -> u64;
    /// Starts a default output buffer through the legacy native adapter.
    pub(super) fn __elephc_eval_ob_start() -> i64;
    /// Returns the number of active native output buffers.
    pub(super) fn __elephc_eval_ob_level() -> i64;
    /// Returns the top buffer length or the no-buffer sentinel.
    pub(super) fn __elephc_eval_ob_length() -> i64;
    /// Copies the top buffer contents into an owned string returned through output slots.
    pub(super) fn __elephc_eval_ob_contents(out_ptr: *mut *const u8, out_len: *mut i64) -> i64;
    /// Returns used bytes and allocated capacity for the requested buffer level.
    pub(super) fn __elephc_eval_ob_stats(index: i64, out_used: *mut i64, out_size: *mut i64)
        -> i64;
    /// Updates native implicit-flush behavior from the supplied boolean flag.
    pub(super) fn __elephc_eval_ob_implicit_flush(enable: i64);
    /// Releases a string returned by an output-buffer contents adapter.
    pub(super) fn __elephc_eval_ob_release_string(ptr: *const u8);
    /// Returns chunk size, operation flags, and started state for a buffer slot.
    pub(super) fn __elephc_eval_ob_slot_meta(
        index: i64,
        out_chunk: *mut i64,
        out_flags: *mut i64,
        out_user_started: *mut i64,
    ) -> i64;
    /// Returns the display-name byte view for the requested active buffer slot.
    pub(super) fn __elephc_eval_ob_slot_name(
        index: i64,
        out_ptr: *mut *const u8,
        out_len: *mut i64,
    ) -> i64;
    /// Installs the invocation and per-buffer retirement callbacks together.
    pub(super) fn __elephc_eval_install_ob_handler_hook(callback: usize, release: usize);
    /// Returns object identity when releasing this boxed owner would retire the object.
    pub(super) fn __elephc_eval_value_final_object_identity(value: *mut RuntimeCell) -> u64;
    /// Consumes one boxed runtime owner and runs native cleanup as needed.
    pub(super) fn __elephc_eval_value_release(value: *mut RuntimeCell);
    /// Adds one owner to the supplied boxed runtime value.
    pub(super) fn __elephc_eval_value_retain(value: *mut RuntimeCell) -> *mut RuntimeCell;
    /// Retains the original boxed handler value installed by compiled AOT code.
    pub(super) fn __elephc_eval_pcntl_aot_signal_handler(signal: i64) -> *mut RuntimeCell;
    /// Installs the optional eval dynamic object destructor callback.
    pub(super) fn __elephc_eval_install_dynamic_object_destructor_hook(callback: usize);
}

/// Forwards one installed eval ob-handler callback address to the generated runtime.
///
/// # Safety
/// `callback` must follow the eval ob-handler ABI; see
/// `crate::runtime_hooks::install_ob_handler_hook`.
pub(super) unsafe fn install_ob_handler_hook_raw(callback: usize, release: usize) {
    unsafe {
        __elephc_eval_install_ob_handler_hook(callback, release);
    }
}
