//! Purpose:
//! Defines registered native function descriptors and their callable signatures.
//!
//! Called from:
//! - FFI registration and native function binding/dispatch.
//!
//! Key details:
//! - Parameter metadata, bridge support, required arity, and return type travel with the invoker.
//! - Only PHP-VISIBLE parameters are registered for a free function: eval reaches one through a
//!   descriptor invoker that synthesizes the compiler-internal `func_args` slots itself. The
//!   registered `NativeCallableShape` is still what states the required arity, because a default
//!   that the eval default ABI cannot represent registers no default at all.

use super::*;
use crate::eval_ir::EvalSourceLocation;

/// Native AOT function callback metadata visible to runtime eval fragments.
#[derive(Clone)]
pub struct NativeFunction {
    pub(super) name: String,
    /// Descriptor pointer held as a word so the table can be shared across threads.
    ///
    /// Generated descriptors are immutable module-level data; keeping the address as
    /// an integer lets one registration snapshot be published behind an `Arc` instead
    /// of deep-copied into every context.
    pub(super) descriptor: usize,
    pub(super) invoker: NativeFunctionInvoker,
    pub(super) param_count: usize,
    pub(super) param_names: Vec<String>,
    pub(super) param_types: Vec<Option<EvalParameterType>>,
    pub(super) param_defaults: Vec<Option<NativeCallableDefault>>,
    pub(super) param_default_constant_names: Vec<Option<String>>,
    pub(super) param_by_ref: Vec<bool>,
    pub(super) variadic_index: Option<usize>,
    pub(super) return_type: Option<EvalParameterType>,
    pub(super) required_param_count: Option<usize>,
    pub(super) returns_reference: bool,
    pub(super) attributes: Vec<EvalAttribute>,
    pub(super) bridge_supported: bool,
    /// Address of the word that says whether a CONDITIONALLY declared function exists yet, or 0
    /// for an unconditional one.
    ///
    /// `if (!function_exists('f')) { function f() {} }` compiles to a variant whose public name
    /// dispatches through a module-global "active variant" slot, which the declaration statement
    /// fills when it runs. php has no `f` until then, and never one when the guard is false, so
    /// eval must answer from that same slot, at every lookup: the registration snapshot is built
    /// once per process, long before most declarations run.
    pub(super) active_slot: usize,
    pub(super) shape: Option<NativeCallableShape>,
    /// File and line range of a compiled closure literal, decoded from its descriptor.
    pub(super) source: Option<(String, EvalSourceLocation)>,
}

impl NativeFunction {
    /// Creates callback metadata for a descriptor-compatible AOT function.
    pub fn new(
        descriptor: *mut c_void,
        invoker: NativeFunctionInvoker,
        param_count: usize,
    ) -> Self {
        Self {
            name: String::new(),
            descriptor: descriptor as usize,
            invoker,
            param_count,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_defaults: Vec::new(),
            param_default_constant_names: Vec::new(),
            param_by_ref: Vec::new(),
            variadic_index: None,
            return_type: None,
            required_param_count: None,
            returns_reference: false,
            attributes: Vec::new(),
            bridge_supported: true,
            active_slot: 0,
            shape: None,
            source: None,
        }
    }

    /// Records the file and line range of the compiled closure literal behind this callback.
    pub fn set_source(&mut self, file: String, location: EvalSourceLocation) {
        self.source = Some((file, location));
    }

    /// Returns the compiled closure literal's line range, when the descriptor carried one.
    pub fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source.as_ref().map(|(_, location)| *location)
    }

    /// Returns the file of the compiled closure literal, when the descriptor carried one.
    pub fn source_file(&self) -> Option<&str> {
        self.source.as_ref().map(|(file, _)| file.as_str())
    }

    /// Returns the lowercase name this callback was registered under, empty until registered.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns declaration attributes retained for native function reflection.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Appends one declaration attribute in source order, including repeated attributes.
    pub fn add_attribute(&mut self, attribute: EvalAttribute) {
        self.attributes.push(attribute);
    }

    /// Returns whether the generated registration identifies a compiler-internal declaration.
    /// Registrations without origin metadata retain user-function semantics.
    pub fn is_internal(&self) -> bool {
        self.shape.is_some_and(|shape| shape.is_internal)
    }

    /// Returns the visible positional parameter count accepted by this callback.
    pub const fn param_count(&self) -> usize {
        self.param_count
    }

    /// Records the PHP parameter name for one positional callback slot.
    pub fn set_param_name(&mut self, index: usize, name: impl Into<String>) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_names.len() < self.param_count {
            self.param_names.resize(self.param_count, String::new());
        }
        self.param_names[index] = name.into();
        true
    }

    /// Returns the PHP-visible parameter names registered for this callback.
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }

    /// Records the PHP declared type metadata for one positional callback slot.
    pub fn set_param_type(&mut self, index: usize, param_type: EvalParameterType) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_types.len() < self.param_count {
            self.param_types.resize(self.param_count, None);
        }
        self.param_types[index] = Some(param_type);
        true
    }

    /// Returns PHP declared parameter types registered for this callback.
    pub fn param_types(&self) -> &[Option<EvalParameterType>] {
        &self.param_types
    }

    /// Returns the registered declared type for one parameter slot, if any.
    pub fn param_type(&self, index: usize) -> Option<&EvalParameterType> {
        self.param_types.get(index).and_then(Option::as_ref)
    }

    /// Records a PHP default value for one positional callback slot.
    pub fn set_param_default(&mut self, index: usize, default: NativeCallableDefault) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_defaults.len() < self.param_count {
            self.param_defaults.resize(self.param_count, None);
        }
        self.param_defaults[index] = Some(default);
        true
    }

    /// Returns the registered default for one parameter slot, if any.
    pub fn param_default(&self, index: usize) -> Option<&NativeCallableDefault> {
        self.param_defaults.get(index).and_then(Option::as_ref)
    }

    /// Records the source constant name without replacing its lazy executable default.
    pub fn set_param_default_constant_name(&mut self, index: usize, name: String) -> bool {
        if index >= self.param_count || self.param_default(index).is_none() || name.is_empty() {
            return false;
        }
        self.param_default_constant_names.resize(self.param_count, None);
        self.param_default_constant_names[index] = Some(name);
        true
    }

    /// Returns the declared constant spelling even when evaluating its default raises Error.
    pub fn param_default_constant_name(&self, index: usize) -> Option<&str> {
        self.param_default_constant_names.get(index).and_then(Option::as_deref)
    }

    /// Records whether one positional callback parameter is by-reference.
    pub fn set_param_by_ref(&mut self, index: usize, by_ref: bool) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_by_ref.len() < self.param_count {
            self.param_by_ref.resize(self.param_count, false);
        }
        self.param_by_ref[index] = by_ref;
        true
    }

    /// Records which positional callback parameter is variadic.
    pub fn set_variadic_index(&mut self, index: usize) -> bool {
        if index >= self.param_count {
            return false;
        }
        self.variadic_index = Some(index);
        true
    }

    /// Records the PHP declared return type metadata for this callback.
    pub fn set_return_type(&mut self, return_type: EvalParameterType) {
        self.return_type = Some(return_type);
    }

    /// Records the exact required arity carried by a runtime callable descriptor.
    pub fn set_required_param_count(&mut self, required_param_count: usize) -> bool {
        if required_param_count > self.param_count {
            return false;
        }
        self.required_param_count = Some(required_param_count);
        true
    }

    /// Records whether eval may dispatch this callback through the generated bridge.
    pub fn set_bridge_supported(&mut self, supported: bool) {
        self.bridge_supported = supported;
    }

    /// Makes this callback exist only while the word at `slot` is nonzero (see `active_slot`).
    pub fn set_active_slot(&mut self, slot: usize) {
        self.active_slot = slot;
    }

    /// Returns whether php would see this function declared right now.
    pub fn is_active(&self) -> bool {
        if self.active_slot == 0 {
            return true;
        }
        // SAFETY: the slot is a module-global, 8-byte `.comm` word of the generated program,
        // registered by generated code and live for the whole process.
        unsafe { std::ptr::read_volatile(self.active_slot as *const u64) != 0 }
    }

    /// Returns whether one registered parameter is by-reference.
    pub fn param_by_ref(&self, index: usize) -> bool {
        self.param_by_ref.get(index).copied().unwrap_or(false)
    }

    /// Returns whether one registered parameter is the variadic parameter.
    pub fn param_variadic(&self, index: usize) -> bool {
        self.variadic_index == Some(index)
    }

    /// Returns the registered declared return type, if any.
    pub fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }

    /// Records that the AOT declaration is `function &name()`.
    pub fn set_returns_reference(&mut self, returns_reference: bool) {
        self.returns_reference = returns_reference;
    }

    /// Returns whether the AOT declaration returns by reference, for
    /// `ReflectionFunction::returnsReference()`.
    pub const fn returns_reference(&self) -> bool {
        self.returns_reference
    }

    /// Returns whether eval may dispatch this callback through the generated bridge.
    pub const fn bridge_supported(&self) -> bool {
        self.bridge_supported
    }

    /// Records the explicit PHP signature shape emitted by the generated registration.
    pub fn set_shape(&mut self, shape: NativeCallableShape) {
        self.shape = Some(shape);
    }

    /// Returns the resolved PHP-visible / compiler-internal partition of the registered slots.
    pub fn frame_shape(&self) -> NativeCallableFrameShape {
        NativeCallableFrameShape::new(self.param_count, self.variadic_index, self.shape)
    }

    /// Returns how many registered slots are PHP-visible NON-variadic parameters.
    pub fn visible_regular_param_count(&self) -> usize {
        self.frame_shape().visible_regular_param_count()
    }

    /// Returns the variadic slot the PHP source itself declared, if the source declared one.
    pub fn source_variadic_index(&self) -> Option<usize> {
        self.frame_shape().source_variadic_index()
    }

    /// Returns the minimum number of arguments this callback requires.
    ///
    /// The registered shape is authoritative, because a declared default whose VALUE the eval
    /// default ABI cannot represent (an enum case, a deeply nested constant expression) registers
    /// no default at all and would otherwise make an optional parameter look required. Only a
    /// registration that emitted no shape falls back to reading the registered defaults.
    pub fn required_param_count(&self) -> usize {
        if let Some(required_param_count) = self.required_param_count {
            return required_param_count;
        }
        let shape = self.frame_shape();
        if let Some(required) = shape.declared_required_param_count() {
            return required;
        }
        (0..shape.visible_regular_param_count())
            .rfind(|index| self.param_default(*index).is_none())
            .map_or(0, |index| index + 1)
    }

    /// Invokes the descriptor-compatible callback with a boxed Mixed arg array.
    ///
    /// # Safety
    /// `arg_array` must be a boxed Mixed indexed array whose elements are boxed
    /// Mixed cells following the descriptor-invoker ABI.
    pub unsafe fn call(&self, arg_array: RuntimeCellHandle) -> RuntimeCellHandle {
        crate::runtime_hooks::request_invoker_boundary();
        // Registered AOT user functions ignore the descriptor ABI's trailing lexical-scope
        // argument. Eval-owned callbacks have their own scope stack, so global is the only
        // honest native invocation-site value at this bridge boundary.
        RuntimeCellHandle::from_raw((self.invoker)(
            self.descriptor as *mut c_void,
            arg_array.as_ptr(),
            -1,
        ))
    }
}
