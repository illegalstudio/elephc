//! Purpose:
//! Defines the runtime value operation contract used by the EvalIR interpreter.
//! The trait abstracts over opaque elephc runtime cells while eval drives PHP semantics.
//!
//! Called from:
//! - `crate::interpreter::execute_program_with_context()`
//! - Eval builtin and expression execution helpers.
//!
//! Key details:
//! - Implementors own allocation, retain/release, casting, arithmetic, and target runtime calls.
//! - Tag constants mirror boxed Mixed runtime tags consumed by eval-only helpers.

use std::ffi::c_void;

use elephc_builtin_contract::RuntimeBuiltinId;

use crate::errors::EvalStatus;
use crate::eval_ir::EvalBinOp;
use crate::value::RuntimeCellHandle;

mod builtins;
pub(crate) use builtins::default_builtin_call;

/// Runtime value hooks required by the EvalIR interpreter.
pub trait RuntimeValueOps {
    /// Calls a typed boxed-cell runtime builtin when this implementation supports it.
    ///
    /// Test and embedding implementations inherit this adapter over the existing
    /// value-operation contract. The generated-runtime adapter overrides it with
    /// the versioned C ABI, so Magician does not select production helpers by name.
    fn runtime_builtin_call(
        &mut self,
        id: RuntimeBuiltinId,
        args: &[RuntimeCellHandle],
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        default_builtin_call(self, id, args)
    }

    /// Transfers a pending exception produced by the boxed builtin runtime to its eval caller.
    fn take_pending_runtime_throwable(&mut self) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        Ok(None)
    }

    /// Creates a runtime indexed-array cell with room for at least `capacity` elements.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime indexed-array cell specialized for direct string payload slots.
    fn string_array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Appends one string payload to a runtime string-array cell.
    fn string_array_push(
        &mut self,
        array: RuntimeCellHandle,
        value: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime associative-array cell with room for at least `capacity` elements.
    fn assoc_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reads one element from a runtime Mixed cell using PHP array-read semantics.
    ///
    /// Missing keys and non-array receivers return PHP null, matching the generated
    /// `__rt_mixed_array_get` runtime helper.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns an owned argument read, preserving a stored PHP reference when supported.
    fn array_get_preserving_references(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.array_get(array, index)
    }

    /// Checks whether a normalized PHP array key exists without conflating null values with misses.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the foreach-visible key at a zero-based iteration position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Writes one element to a runtime array-like Mixed cell and returns the target cell.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reads one insertion-order element using the same ownership contract as array_get.
    /// Providers with distinct numeric string keys must override the normalized-key fallback.
    fn array_iter_value(&mut self, array: RuntimeCellHandle, position: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.array_iter_key(array, position)?;
        self.array_get(array, key)
    }

    /// Creates an independent PHP array value for a by-value variable assignment.
    /// Native implementations may share the payload until an ordinary COW write.
    fn copy_array_value(
        &mut self,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.array_clone_shallow(array)
    }

    /// Creates a shallow array copy before writeback paths perform PHP COW writes.
    fn array_clone_shallow(
        &mut self,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let len = self.array_len(array)?;
        let tag = self.type_tag(array)?;
        let mut result = match tag {
            EVAL_TAG_ARRAY => self.array_new(len)?,
            EVAL_TAG_ASSOC => self.assoc_new(len)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        for position in 0..len {
            let key = self.array_iter_key(array, position)?;
            let value = self.array_get(array, key)?;
            result = self.array_set(result, key, value)?;
        }
        if tag == EVAL_TAG_ASSOC { self.array_copy_index_history(array, result)?; }
        Ok(result)
    }

    /// Reads a named property from a runtime object held in a boxed Mixed cell.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Checks whether a generated/AOT instance property is initialized.
    fn property_is_initialized(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<bool, EvalStatus>;

    /// Writes a named property on a runtime object held in a boxed Mixed cell.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus>;

    /// Reads a generated/AOT static property through the generated bridge.
    fn static_property_get(
        &mut self,
        class_name: &str,
        property: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus>;

    /// Checks whether a generated/AOT static property is initialized.
    fn static_property_is_initialized(
        &mut self,
        class_name: &str,
        property: &str,
    ) -> Result<bool, EvalStatus>;

    /// Writes a generated/AOT static property through the generated bridge.
    fn static_property_set(
        &mut self,
        class_name: &str,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<bool, EvalStatus>;

    /// Reads a generated/AOT class-like constant through the generated bridge.
    fn class_constant_get(
        &mut self,
        class_name: &str,
        constant: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus>;

    /// Creates a shallow clone of a runtime object held in a boxed Mixed cell.
    fn object_clone_shallow(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the number of public JSON-visible properties on a runtime object.
    fn object_property_len(&mut self, object: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Returns the public property key at a zero-based JSON object iteration position.
    fn object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Calls a named method on a runtime object held in a boxed Mixed cell.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Calls a named static method through the generated AOT bridge.
    fn static_method_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Converts a generated native call result into an eval result or pending throwable status.
    fn native_call_result(
        &mut self,
        result: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        if result.is_null() {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(result)
        }
    }

    /// Materializes a synthetic `ReflectionAttribute` object through generated private-layout code.
    fn reflection_attribute_new(
        &mut self,
        name: &str,
        args: RuntimeCellHandle,
        target: u64,
        repeated: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Materializes a synthetic ReflectionClass/Method/Property object through generated private-layout code.
    fn reflection_owner_new(
        &mut self,
        owner_kind: u64,
        reflected_name: &str,
        attrs: RuntimeCellHandle,
        interface_names: RuntimeCellHandle,
        trait_names: RuntimeCellHandle,
        method_names: RuntimeCellHandle,
        property_names: RuntimeCellHandle,
        method_objects: RuntimeCellHandle,
        property_objects: RuntimeCellHandle,
        parent_class: RuntimeCellHandle,
        flags: u64,
        modifiers: u64,
        method_modifiers: u64,
        constant_value: RuntimeCellHandle,
        backing_value: RuntimeCellHandle,
        constructor: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns generated AOT ReflectionMethod flags for a class/method pair.
    fn reflection_method_flags(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<u64>, EvalStatus>;

    /// Returns the generated AOT declaring class for a class/method pair.
    fn reflection_method_declaring_class(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<String>, EvalStatus>;

    /// Returns generated AOT ReflectionMethod names visible for one class.
    fn reflection_method_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the generated program source file used for AOT reflection metadata.
    fn reflection_source_file(&mut self) -> Result<Option<String>, EvalStatus> {
        Ok(None)
    }

    /// Returns generated AOT ReflectionClass modifier flags for one class.
    fn reflection_class_flags(&mut self, class_name: &str) -> Result<Option<u64>, EvalStatus>;

    /// Returns generated AOT ReflectionProperty flags for a class/property pair.
    fn reflection_property_flags(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<u64>, EvalStatus>;

    /// Returns the generated AOT declaring class for a class/property pair.
    fn reflection_property_declaring_class(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<String>, EvalStatus>;

    /// Returns generated AOT ReflectionProperty names visible for one class.
    fn reflection_property_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns generated AOT ReflectionClassConstant values without visibility checks.
    fn reflection_constant_value(
        &mut self,
        class_name: &str,
        constant_name: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus>;

    /// Returns generated AOT ReflectionClassConstant flags for a class/constant pair.
    fn reflection_constant_flags(
        &mut self,
        class_name: &str,
        constant_name: &str,
    ) -> Result<Option<u64>, EvalStatus>;

    /// Returns the generated AOT declaring class for a class/constant pair.
    fn reflection_constant_declaring_class(
        &mut self,
        class_name: &str,
        constant_name: &str,
    ) -> Result<Option<String>, EvalStatus>;

    /// Returns generated AOT ReflectionClassConstant names visible for one class.
    fn reflection_constant_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns generated/AOT interface names visible for one reflected class-like symbol.
    fn reflection_class_interface_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns generated/AOT trait names visible for one reflected class-like symbol.
    fn reflection_class_trait_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns generated/AOT trait alias names visible for one reflected class-like symbol.
    fn reflection_class_trait_alias_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns generated/AOT trait alias sources visible for one reflected class-like symbol.
    fn reflection_class_trait_alias_sources(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a named runtime object without constructor arguments.
    fn new_object(&mut self, class_name: &str) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Calls the runtime constructor for an object when the class declares one.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus>;

    /// Returns whether a runtime class table contains the requested class name.
    fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime interface table contains the requested interface name.
    fn interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime trait table contains the requested trait name.
    fn trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime enum table contains the requested enum name.
    fn enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Tests whether a boxed object cell satisfies a class/interface relation.
    fn object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus>;

    /// Returns the PHP-visible runtime class name for an object cell.
    fn object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the PHP-visible parent class name for an object or class-name cell.
    fn parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the visible element count for an array-like runtime cell.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Borrows the host's persistent next integer index; None reports an exhausted append.
    fn array_next_index(&mut self, array: RuntimeCellHandle) -> Result<Option<i64>, EvalStatus>;

    /// Copies exact append history into a freshly reconstructed associative array.
    fn array_copy_index_history(&mut self, source: RuntimeCellHandle, destination: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Returns whether a runtime cell can be indexed like an array by eval writes.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime cell holds PHP null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns the concrete boxed Mixed runtime tag after unwrapping nested Mixed cells.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Creates an invoker-only by-reference marker for a staged Mixed slot.
    fn invoker_ref_cell(
        &mut self,
        slot: *mut RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates an invoker-only by-reference marker for a staged raw one-word slot.
    fn invoker_raw_ref_cell(
        &mut self,
        slot: *mut c_void,
        source_tag: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Extracts the low raw payload word from a boxed Mixed cell.
    fn raw_value_word(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Extracts the high raw payload word from a boxed Mixed cell.
    fn raw_value_high_word(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Duplicates one raw string payload so a staged native by-ref slot owns it.
    fn retain_raw_string_words(&mut self, ptr: u64, len: u64) -> Result<(u64, u64), EvalStatus>;

    /// Boxes one raw string payload as a Mixed string cell.
    fn raw_string_value(&mut self, ptr: u64, len: u64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Releases one raw string payload owned by a staged native by-ref slot.
    fn release_raw_string_words(&mut self, ptr: u64, len: u64) -> Result<(), EvalStatus>;

    /// Retains one raw heap payload word so a staged native by-ref slot owns it.
    fn retain_raw_heap_word(&mut self, word: u64) -> Result<u64, EvalStatus>;

    /// Boxes one one-word raw payload as a Mixed cell with the provided runtime tag.
    fn raw_word_value(
        &mut self,
        source_tag: u64,
        word: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Boxes one raw heap payload word as a Mixed cell after inspecting its heap kind.
    fn raw_heap_word_value(&mut self, word: u64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Releases one raw heap payload word owned by a staged native by-ref slot.
    fn release_raw_heap_word(&mut self, word: u64) -> Result<(), EvalStatus>;

    /// Returns the unboxed object payload pointer used for PHP object identity.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Returns the PHP OBJECT HANDLE — the small dense integer `spl_object_id()`
    /// reports and `var_dump()` prints as `object(C)#N`. This is deliberately not
    /// `object_identity`: that one is the storage address, which destructor
    /// bookkeeping keys on, while this one must match the AOT engine's pool.
    fn php_object_handle(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Returns the object identity that would be freed by releasing this owned cell, if any.
    fn final_object_identity_for_release(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<Option<u64>, EvalStatus>;

    /// Releases one owned runtime cell that is no longer held by the eval scope.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Collects unreachable native cycles at an explicit root-removal safe point.
    fn collect_cycles(&mut self) -> Result<(), EvalStatus> { Ok(()) }

    /// Retains one runtime cell so the eval caller receives an independent owner.
    fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reports whether this host supports GC-owned references independent of eval scope metadata.
    fn supports_persistent_references(&self) -> bool { false }

    /// Reports whether a boxed value is a writable persistent PHP reference.
    fn is_reference(&mut self, _value: RuntimeCellHandle) -> Result<bool, EvalStatus> { Ok(false) }

    /// Creates one owned reference containing an independent copy of a borrowed PHP value.
    fn reference_new(&mut self, _value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Err(EvalStatus::UnsupportedConstruct)
    }

    /// Replaces a reference's value by copy and transfers its previous owner without releasing it.
    fn reference_replace(&mut self, _reference: RuntimeCellHandle, _value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Err(EvalStatus::UnsupportedConstruct)
    }

    /// Copies a PHP value independently from any persistent reference wrapping it.
    fn copy_value(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> { self.retain(value) }

    /// Emits or suppresses one PHP runtime warning through the target runtime.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus>;

    /// Emits one unsuppressible PHP fatal diagnostic and stops eval execution.
    fn fatal(&mut self, message: &str) -> Result<(), EvalStatus> {
        self.warning(message)?;
        Err(EvalStatus::RuntimeFatal)
    }

    /// Publishes whether eval is invoking a signal handler so Fiber switches can be rejected.
    fn set_pcntl_dispatching(&mut self, active: bool) -> Result<(), EvalStatus> {
        crate::context::pcntl_runtime::set_fiber_dispatching(active);
        Ok(())
    }

    /// Retains an AOT-installed signal handler value for eval introspection when available.
    fn pcntl_aot_signal_handler(
        &mut self,
        _signal: i64,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        Ok(None)
    }

    /// Creates a runtime null cell.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime bool cell.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime int cell.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime resource cell with a zero-based native resource payload.
    fn resource(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime cell for an eval-owned incremental hash context.
    ///
    /// SEPARATE FROM `resource()` ON PURPOSE. PHP 8's `hash_init()` returns a
    /// `HashContext` OBJECT, which draws from the object-handle space and consumes
    /// nothing from the per-request RESOURCE counter that `get_resource_id()` and
    /// `var_dump()` report. Routing hash contexts through `resource()` made
    /// `eval('hash_init("md5"); $x = fopen(...);')` report an id one higher than PHP
    /// for `$x`, and shifted the host program's later `fopen()`s too, because eval and
    /// the compiled program share one id counter.
    ///
    /// The cell still carries runtime tag 9, because that is the shape
    /// `eval_resource_payload` reads the table key back out of; what changes is the
    /// resource KIND word, which becomes 5 = eval-owned inert handle: no PHP id is
    /// bound and no destructor runs. `value` is a key into `EvalStreamResources`, which
    /// owns the real `elephc_crypto` handle and frees it in its own `Drop` — this cell
    /// must never free anything.
    fn hash_context(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime cell for an eval-owned curl easy/multi/share handle.
    ///
    /// A DEFAULT METHOD, not a new generated-runtime wrapper: PHP 8's `CurlHandle` /
    /// `CurlMultiHandle` / `CurlShareHandle` are OBJECTS that consume nothing from the
    /// resource counter, exactly like `HashContext` above, and the box shape that fact
    /// requires — runtime tag 9, resource kind 5, no PHP id, no destructor — is already
    /// exactly what `hash_context()` produces. Reusing it here means curl needs no new
    /// entry in the generated `__elephc_eval_value_*` wrapper family
    /// (`elephc::codegen_support::runtime::eval_bridge`) at all. `value` is a key into
    /// `EvalStreamResources`'s curl tables (`crate::stream_resources::curl`, behind the
    /// `curl` Cargo feature), which own the real `elephc_curl_*` handle and free it in
    /// `EvalStreamResources`'s own `Drop`.
    fn curl_handle(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        self.hash_context(value)
    }

    /// Creates a runtime float cell.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime string cell.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a PHP literal with interned logical identity when the runtime models string origins.
    fn string_literal(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> { self.string(value) }

    /// Creates an interned PHP literal from raw bytes without assuming valid UTF-8.
    fn string_literal_bytes(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus> { self.string_bytes_value(value) }

    /// Creates a runtime byte-string cell from raw PHP string bytes.
    fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP integer cell.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP float cell.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP string cell.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP boolean cell.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `abs()` for one runtime cell while preserving integer/float result typing.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `ceil()` for one runtime cell after PHP numeric conversion.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `floor()` for one runtime cell after PHP numeric conversion.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `sqrt()` for one runtime cell after PHP numeric conversion.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reverses a string value using PHP `strrev()` byte-string semantics.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Divides two runtime cells using PHP `fdiv()` semantics.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes the floating-point remainder using PHP `fmod()` semantics.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Adds two runtime cells using PHP addition semantics.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Subtracts two runtime cells using PHP numeric semantics.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Multiplies two runtime cells using PHP numeric semantics.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Divides two runtime cells using PHP numeric semantics.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes modulo for two runtime cells using PHP integer modulo semantics.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Raises one runtime cell to another using PHP exponentiation semantics.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Rounds one runtime cell using PHP `round()` semantics and optional precision.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Applies an integer bitwise or shift operation to two runtime cells.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Applies integer bitwise NOT to one runtime cell.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Concatenates two runtime cells using PHP string conversion semantics.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two runtime cells and returns a boxed PHP boolean cell.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two runtime cells and returns a boxed PHP spaceship integer.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two normalized PHP array keys with native `SORT_REGULAR` semantics.
    fn regular_key_compare(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<i64, EvalStatus>;

    /// Emits one runtime cell to stdout using PHP echo semantics.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Casts one runtime cell to a PHP string and copies its bytes for parsing.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus>;

    /// Converts one runtime cell to PHP boolean truthiness.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Pushes a new output buffer with handler metadata; false when refused.
    /// `handler_id` is a magician ob-handler registry id (None = default handler),
    /// `name` the PHP-visible display name, `chunk_size`/`flags` PHP's ob_start
    /// parameters.
    fn ob_start_ex(
        &mut self,
        handler_id: Option<u64>,
        name: &str,
        chunk_size: i64,
        flags: i64,
    ) -> Result<bool, EvalStatus>;

    /// Pops the top output buffer and returns its raw bytes (handler applied and
    /// contents flushed to the parent sink when `flush`); None when refused.
    fn ob_get_end(&mut self, flush: bool) -> Result<Option<Vec<u8>>, EvalStatus>;

    /// Returns `(chunk_size, stored_flags, is_user_handler, started)` for the
    /// 0-based buffer index, if it exists.
    fn ob_slot_meta(&mut self, index: i64)
        -> Result<Option<(i64, i64, bool, bool)>, EvalStatus>;

    /// Returns a copy of one buffer's handler display name, if the slot exists.
    fn ob_slot_name(&mut self, index: i64) -> Result<Option<Vec<u8>>, EvalStatus>;

    /// Returns the current output-buffer nesting depth (0 = no buffering).
    fn ob_level(&mut self) -> Result<i64, EvalStatus>;

    /// Returns the top output buffer's byte count, or None when no buffer is active.
    fn ob_length(&mut self) -> Result<Option<i64>, EvalStatus>;

    /// Returns a copy of the top output buffer's bytes, or None when no buffer is active.
    fn ob_contents(&mut self) -> Result<Option<Vec<u8>>, EvalStatus>;

    /// Truncates the top output buffer in place; false when no buffer is active.
    fn ob_clean(&mut self) -> Result<bool, EvalStatus>;

    /// Flushes the top output buffer to the parent sink without popping it.
    fn ob_flush(&mut self) -> Result<bool, EvalStatus>;

    /// Pops the top output buffer, flushing it to the parent sink when `flush` is true.
    fn ob_end(&mut self, flush: bool) -> Result<bool, EvalStatus>;

    /// Returns `(buffer_used, buffer_size)` for the 0-based buffer index, if it exists.
    fn ob_stats(&mut self, index: i64) -> Result<Option<(i64, i64)>, EvalStatus>;

    /// Stores the (semantically inert) implicit-flush flag.
    fn ob_implicit_flush(&mut self, enable: bool) -> Result<(), EvalStatus>;
}

pub(super) const EVAL_TAG_INT: u64 = 0;
pub(super) const EVAL_TAG_STRING: u64 = 1;
pub(super) const EVAL_TAG_FLOAT: u64 = 2;
pub(super) const EVAL_TAG_BOOL: u64 = 3;
pub(super) const EVAL_TAG_ARRAY: u64 = 4;
pub(super) const EVAL_TAG_ASSOC: u64 = 5;
pub(super) const EVAL_TAG_OBJECT: u64 = 6;
pub(super) const EVAL_TAG_MIXED: u64 = 7;
pub(super) const EVAL_TAG_NULL: u64 = 8;
pub(super) const EVAL_TAG_RESOURCE: u64 = 9;
pub(super) const EVAL_TAG_CALLABLE: u64 = 10;
pub(super) const EVAL_TAG_INVOKER_REF_CELL: u64 = 11;

pub(super) const EVAL_REFLECTION_OWNER_CLASS: u64 = 0;
pub(super) const EVAL_REFLECTION_OWNER_METHOD: u64 = 1;
pub(super) const EVAL_REFLECTION_OWNER_PROPERTY: u64 = 2;
pub(super) const EVAL_REFLECTION_OWNER_CLASS_CONSTANT: u64 = 3;
pub(super) const EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE: u64 = 4;
pub(super) const EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE: u64 = 5;
pub(super) const EVAL_REFLECTION_OWNER_PARAMETER: u64 = 6;
pub(super) const EVAL_REFLECTION_OWNER_NAMED_TYPE: u64 = 7;
pub(super) const EVAL_REFLECTION_OWNER_UNION_TYPE: u64 = 8;
pub(super) const EVAL_REFLECTION_OWNER_INTERSECTION_TYPE: u64 = 9;
pub(super) const EVAL_REFLECTION_OWNER_FUNCTION: u64 = 10;
pub(super) const EVAL_REFLECTION_OWNER_ENUM: u64 = 11;
pub(super) const EVAL_REFLECTION_OWNER_OBJECT: u64 = 12;
