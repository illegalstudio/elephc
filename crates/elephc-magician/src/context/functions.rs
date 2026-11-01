//! Purpose:
//! Registers dynamic constants, functions, closures, and native function metadata.
//!
//! Called from:
//! - Declaration execution, closure creation, and dynamic function dispatch.
//!
//! Key details:
//! - Synthetic closure identities and native parameter metadata remain context-local.

use super::*;

/// One scalar AOT global constant registered with a persistent eval context.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalNativeGlobalConstant {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// Binary PHP string constant with a payload that is not valid UTF-8.
    Bytes(Vec<u8>),
    Resource(i64),
}

/// One AOT *user-declared* global constant registered with a persistent eval context.
///
/// This is a separate registry from [`EvalNativeGlobalConstant`] on purpose: PHP reports
/// user constants under the `user` category, and the values are compiler metadata rather
/// than runtime cells, so nothing here may be merged into the dynamic `define()` map that
/// the native constant inventory exports back to AOT.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalNativeUserConstant {
    /// A scalar constant, sharing the scalar shapes of the Core registry.
    Scalar(EvalNativeGlobalConstant),
    /// A recursively nested array constant, keyed exactly as AOT materialized it.
    Array(Vec<NativeCallableArrayDefaultElement>),
}

impl ElephcEvalContext {
    /// Registers one AOT global constant for eval fetches and Core introspection.
    pub fn define_native_global_constant(
        &mut self,
        name: &str,
        value: EvalNativeGlobalConstant,
    ) -> bool {
        let key = normalize_constant_name(name);
        if key.is_empty() || self.native_global_constants.contains_key(&key) {
            return false;
        }
        self.native_global_constants.insert(key, value);
        true
    }

    /// Returns one registered AOT global constant by its case-sensitive PHP name.
    pub fn native_global_constant(&self, name: &str) -> Option<&EvalNativeGlobalConstant> {
        self.native_global_constants
            .get(&normalize_constant_name(name))
    }

    /// Returns whether an AOT global constant is registered under the requested name.
    pub fn has_native_global_constant(&self, name: &str) -> bool {
        self.native_global_constant(name).is_some()
    }

    /// Returns registered AOT global constants in stable PHP-visible name order.
    pub(crate) fn native_global_constant_entries(
        &self,
    ) -> Vec<(String, EvalNativeGlobalConstant)> {
        let mut entries = self
            .native_global_constants
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    /// Registers one AOT user-declared global constant for eval fetches and introspection.
    ///
    /// Fails when the name is empty or already registered in either native registry, so a
    /// seeded name can never be duplicated by a second registration call. A name already
    /// present in the dynamic `define()` map is rejected too, and left untouched, so a late
    /// seeding call can never shadow or replace a constant a fragment already defined.
    pub fn define_native_user_constant(
        &mut self,
        name: &str,
        value: EvalNativeUserConstant,
    ) -> bool {
        let key = normalize_constant_name(name);
        if key.is_empty()
            || self.native_user_constants.contains_key(&key)
            || self.native_global_constants.contains_key(&key)
            || self.constants.contains_key(&key)
        {
            return false;
        }
        self.native_user_constants.insert(key, value);
        true
    }

    /// Returns one registered AOT user constant by its case-sensitive PHP name.
    pub fn native_user_constant(&self, name: &str) -> Option<&EvalNativeUserConstant> {
        self.native_user_constants
            .get(&normalize_constant_name(name))
    }

    /// Returns whether an AOT user constant is registered under the requested name.
    pub fn has_native_user_constant(&self, name: &str) -> bool {
        self.native_user_constant(name).is_some()
    }

    /// Returns registered AOT user constants in stable PHP-visible name order.
    pub(crate) fn native_user_constant_entries(&self) -> Vec<(String, EvalNativeUserConstant)> {
        let mut entries = self
            .native_user_constants
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    /// Defines an eval dynamic constant value, failing if the name is invalid or already present.
    pub fn define_constant(&mut self, name: &str, value: RuntimeCellHandle) -> bool {
        let key = normalize_constant_name(name);
        if key.is_empty()
            || self.has_constant(&key)
            || self.native_global_constants.contains_key(&key)
            || self.native_user_constants.contains_key(&key)
        {
            return false;
        }
        // Constants are request-global in php; see `register_global_eval_constant`.
        #[cfg(not(test))]
        if !crate::context::register_global_eval_constant(&key, self as *mut Self) {
            return false;
        }
        self.constants.insert(key, value);
        true
    }

    /// Returns true when this eval context, or any other live one, defined the constant.
    pub fn has_constant(&self, name: &str) -> bool {
        self.constant(name).is_some()
    }

    /// Returns an eval dynamic constant value by case-sensitive PHP constant name.
    ///
    /// Falls back to the context that `define()`d it when that is another one: an autoloader runs
    /// the files it includes in its own context, and their constants are the whole request's.
    pub fn constant(&self, name: &str) -> Option<RuntimeCellHandle> {
        let key = normalize_constant_name(name);
        if let Some(value) = self.constants.get(&key) {
            return Some(value.borrowed());
        }
        #[cfg(not(test))]
        {
            crate::context::global_eval_constant(&key, self as *const Self)
                .map(RuntimeCellHandle::borrowed)
        }
        #[cfg(test)]
        None
    }

    /// Returns user-defined constants in stable name order for PHP introspection.
    pub(crate) fn defined_constant_entries(&self) -> Vec<(String, RuntimeCellHandle)> {
        let mut entries = self
            .constants
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    /// Borrows one dynamic constant in stable name order for a native inventory snapshot.
    pub(crate) fn constant_inventory_entry(&self, index: usize) -> Option<(&str, RuntimeCellHandle)> {
        let mut entries = self.constants.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        entries.get(index).map(|(name, cell)| (name.as_str(), **cell))
    }

    /// Borrows one eval-only function name, excluding native registrations and closures.
    pub(crate) fn function_inventory_entry(&self, index: usize) -> Option<&str> {
        let mut names = self.functions.keys().collect::<Vec<_>>();
        names.sort();
        names.get(index).map(|name| name.as_str())
    }

    /// Returns eval and generated native user-function names in stable order.
    pub(crate) fn defined_user_function_names(&self) -> Vec<String> {
        let mut names = self.functions.keys().cloned().collect::<Vec<_>>();
        names.extend(self.native_functions.keys().cloned());
        names.extend(self.local_native_functions.keys().cloned());
        names.sort();
        names.dedup();
        names
    }

    /// Defines a dynamic user function, failing if the name already exists.
    pub fn define_function(
        &mut self,
        name: impl Into<String>,
        function: EvalFunction,
    ) -> Result<(), EvalFunction> {
        let name = name.into();
        if self.functions.contains_key(&name) || self.has_active_native_function(&name) {
            return Err(function);
        }
        #[cfg(not(test))]
        if !crate::context::register_global_eval_function(&name, self as *mut Self) {
            return Err(function);
        }
        self.functions.insert(name, function);
        Ok(())
    }

    /// Replaces the cell one capture of a registered closure holds, returning the one it held.
    ///
    /// Returns `None` when the closure or the capture is unknown, and hands `cell` back when it
    /// is already the one held, so the caller always releases exactly what it gets.
    pub fn replace_closure_capture_value(
        &mut self,
        closure_name: &str,
        capture_name: &str,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let capture = self
            .closures
            .get_mut(closure_name)?
            .captures
            .iter_mut()
            .find(|capture| capture.name == capture_name)?;
        Some(std::mem::replace(&mut capture.value, cell))
    }

    /// Stores one eval closure instance under a context-local synthetic callable name.
    pub fn define_closure(&mut self, closure: EvalClosure) -> String {
        let name = format!("{{closure:eval:{}}}", self.next_closure_id);
        self.next_closure_id += 1;
        // Stamped here because this is the one place that knows the object is new: every call
        // mints a fresh name, which is exactly php's "one static slot per closure object".
        let mut closure = closure;
        closure.set_slot_key(name.clone());
        self.closures.insert(name.clone(), closure);
        name
    }

    /// Associates a PHP `Closure` object identity with an eval closure callable name.
    pub fn register_closure_object(&mut self, identity: u64, closure_name: &str) {
        self.register_closure_object_target(
            identity,
            EvalClosureObjectTarget::Named(closure_name.to_string()),
        );
    }

    /// Associates a PHP `Closure` object identity with any eval callable target.
    pub fn register_closure_object_target(
        &mut self,
        identity: u64,
        target: EvalClosureObjectTarget,
    ) {
        self.may_have_foreign_pcntl_callables |= target.contains_foreign_context();
        if self.closure_objects.insert(identity, target).is_none() {
            self.live_closure_objects
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        crate::ffi::dynamic_destructors::register_dynamic_object_context(
            identity,
            self as *mut Self,
        );
    }

    /// Returns the callable target bound to a PHP `Closure` object.
    pub fn closure_object_target(&self, identity: u64) -> Option<&EvalClosureObjectTarget> {
        self.closure_objects.get(&identity)
    }

    /// Returns the eval closure callable name bound to a literal PHP `Closure` object.
    pub fn closure_object_name(&self, identity: u64) -> Option<&str> {
        self.closure_objects
            .get(&identity)
            .and_then(|target| match target {
                EvalClosureObjectTarget::Named(name)
                | EvalClosureObjectTarget::BoundNamed { name, .. } => Some(name.as_str()),
                _ => None,
            })
    }

    /// Marks disposal as pending and reports whether no request-global value still owns this context.
    ///
    /// A closure or a function declared by a dynamic include can outlive the AOT frame that
    /// executed that include. Their executable metadata remains in this context until the
    /// associated object is released or the web request reaches its reset boundary.
    pub fn request_retained_context_free(&self) -> bool {
        self.retained_context_free_requested
            .store(true, std::sync::atomic::Ordering::Release);
        !self.has_retained_request_lifetime()
    }

    /// Removes a released Closure object and reports whether it completes a deferred context free.
    pub fn forget_closure_object(&mut self, identity: u64) -> bool {
        if self.closure_objects.remove(&identity).is_none() {
            return false;
        }
        crate::ffi::dynamic_destructors::unregister_dynamic_object(identity);
        let previous = self
            .live_closure_objects
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        debug_assert!(previous > 0, "closure object lifetime count must not underflow");
        previous == 1 && self.should_finalize_retained_context()
    }

    /// Releases one eval-class object lifetime and reports whether deferred context disposal can finish.
    pub(crate) fn forget_dynamic_object_owner(&self) -> bool {
        let previous = self
            .live_dynamic_objects
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        debug_assert!(previous > 0, "dynamic object lifetime count must not underflow");
        previous == 1 && self.should_finalize_retained_context()
    }

    /// Adds one request-global dynamic-function owner to this context.
    pub(crate) fn retain_global_function(&self) {
        self.live_global_functions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Removes one request-global dynamic-function owner and reports deferred-finalization state.
    pub(crate) fn forget_global_function(&self) -> bool {
        let previous = self
            .live_global_functions
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        debug_assert!(previous > 0, "global function lifetime count must not underflow");
        previous == 1 && self.should_finalize_retained_context()
    }

    /// Retains this context while its request-global SPL autoload table is non-empty.
    pub(crate) fn retain_autoload_context(&self) {
        self.live_autoload_contexts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Releases the request-global SPL autoload ownership and reports finalization state.
    pub(crate) fn forget_autoload_context(&self) -> bool {
        let previous = self
            .live_autoload_contexts
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        debug_assert!(previous > 0, "autoload context lifetime count must not underflow");
        previous == 1 && self.should_finalize_retained_context()
    }

    /// Reports whether the context must outlive its originating AOT frame.
    pub(crate) fn has_retained_request_lifetime(&self) -> bool {
        self.live_closure_objects
            .load(std::sync::atomic::Ordering::Acquire)
            != 0
            || self
                .live_dynamic_objects
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
            || self
                .live_global_functions
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
            || self
                .live_autoload_contexts
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
    }

    /// Reports whether a prior frame cleanup may now finalize this retained context.
    fn should_finalize_retained_context(&self) -> bool {
        self.retained_context_free_requested
            .load(std::sync::atomic::Ordering::Acquire)
            && !self.has_retained_request_lifetime()
    }

    /// Defines a generated native function callback, failing if the name already exists.
    pub fn define_native_function(
        &mut self,
        name: impl Into<String>,
        mut function: NativeFunction,
    ) -> Result<(), NativeFunction> {
        let name = name.into();
        if self.functions.contains_key(&name)
            || self.native_functions.contains_key(&name)
            || self.local_native_functions.contains_key(&name)
        {
            return Err(function);
        }
        function.name.clone_from(&name);
        // The module registration fills a table nobody else holds yet, and the shape/param
        // setters below expect to find those entries in it. Once the table is the shared AOT
        // snapshot, a late registration must not copy it; see `local_native_functions`.
        // A fresh context's table is the thread's shared EMPTY value, which `make_mut` copies
        // for nothing; it must stay the registration target so the publish sees the entries.
        if !self.native_functions.is_empty()
            && matches!(self.native_functions, SharedNative::Shared(_))
        {
            self.local_native_functions.insert(name, function);
        } else {
            self.native_functions.insert(name, function);
        }
        Ok(())
    }

    /// Returns a dynamic user function by its lowercase PHP function name.
    pub fn function(&self, name: &str) -> Option<&EvalFunction> {
        self.functions.get(name)
    }

    /// Adopts a function another eval context declared, so this one can call it.
    ///
    /// PHP's function table is process-global; elephc's contexts are per compiled frame that
    /// evals or includes. `sync_global_eval_classes` already imports class-likes across that
    /// boundary, and this is the same import for a function.
    ///
    /// Known divergence: `static` locals are keyed per context, so a function adopted by two
    /// contexts gets one static scope in each where PHP has one. The alternative was
    /// `call to undefined function`, and the shape this exists for -- a Composer `files`
    /// autoload shim like `trigger_deprecation()` -- declares no statics.
    ///
    /// Registration is deliberately NOT repeated: the global table already names the declaring
    /// context as the owner, and re-registering would report a redeclaration.
    #[cfg(not(test))]
    pub(crate) fn adopt_global_function(&mut self, name: &str) -> bool {
        if self.functions.contains_key(name) || self.has_active_native_function(name) {
            return true;
        }
        let Some(function) = crate::context::global_eval_function_declaration(name) else {
            return false;
        };
        self.functions.insert(name.to_string(), function);
        true
    }

    /// Keeps unit-test builds independent from the process-global function registry.
    #[cfg(test)]
    pub(crate) fn adopt_global_function(&mut self, _name: &str) -> bool {
        false
    }

    /// Returns a dynamic eval closure by its synthetic callable name.
    pub fn closure(&self, name: &str) -> Option<&EvalClosure> {
        self.closures.get(name)
    }

    /// Returns a native AOT function callback by its lowercase PHP function name.
    ///
    /// A conditionally declared function whose declaration has not run is not there, exactly as
    /// in php (see `NativeFunction::active_slot`).
    pub fn native_function(&self, name: &str) -> Option<NativeFunction> {
        self.local_native_functions
            .get(name)
            .or_else(|| self.native_functions.get(name))
            .filter(|function| function.is_active())
            .cloned()
    }

    /// Returns whether a native AOT function of this lowercase name is declared right now.
    pub(crate) fn has_active_native_function(&self, name: &str) -> bool {
        self.local_native_functions
            .get(name)
            .or_else(|| self.native_functions.get(name))
            .is_some_and(NativeFunction::is_active)
    }

    /// Records the slot a conditionally declared native AOT callback's existence is read from.
    pub fn define_native_function_active_slot(&mut self, function_name: &str, slot: usize) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| {
                function.set_active_slot(slot);
                true
            })
    }

    /// Records the explicit PHP signature shape for an already registered native AOT callback.
    ///
    /// Emitted from the AST-level signature, so it states the source arity even when a declared
    /// default has no representation in the eval default ABI and is therefore never registered.
    pub fn define_native_function_shape(
        &mut self,
        function_name: &str,
        shape: NativeCallableShape,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .map(|function| function.set_shape(shape))
            .is_some()
    }

    /// Records one parameter name for an already registered native AOT callback.
    pub fn define_native_function_param(
        &mut self,
        function_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| function.set_param_name(index, param_name))
    }

    /// Records one parameter type for an already registered native AOT callback.
    pub fn define_native_function_param_type(
        &mut self,
        function_name: &str,
        index: usize,
        param_type: EvalParameterType,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| function.set_param_type(index, param_type))
    }

    /// Records one parameter default for an already registered native AOT callback.
    pub fn define_native_function_param_default(
        &mut self,
        function_name: &str,
        index: usize,
        default: NativeCallableDefault,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| function.set_param_default(index, default))
    }

    /// Retains source default names separately from native executable values and Error data.
    pub fn define_native_function_param_default_constant_name(
        &mut self, function_name: &str, index: usize, name: String,
    ) -> bool {
        self.native_functions.get_mut(function_name)
            .is_some_and(|function| function.set_param_default_constant_name(index, name))
    }

    /// Records whether one native AOT callback parameter is by-reference.
    pub fn define_native_function_param_by_ref(
        &mut self,
        function_name: &str,
        index: usize,
        by_ref: bool,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| function.set_param_by_ref(index, by_ref))
    }

    /// Records which native AOT callback parameter is variadic.
    pub fn define_native_function_variadic_param(
        &mut self,
        function_name: &str,
        index: usize,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| function.set_variadic_index(index))
    }

    /// Retains one attribute on an already registered native AOT function.
    pub fn define_native_function_attribute(&mut self, function_name: &str, attribute: EvalAttribute) -> bool {
        let key = function_name.trim_start_matches('\\').to_ascii_lowercase();
        self.native_functions.get_mut(&key).is_some_and(|function| {
            function.add_attribute(attribute);
            true
        })
    }

    /// Records that one native AOT callback is declared `function &name()`.
    pub fn define_native_function_returns_reference(&mut self, function_name: &str) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| {
                function.set_returns_reference(true);
                true
            })
    }

    /// Records one native AOT callback return type.
    pub fn define_native_function_return_type(
        &mut self,
        function_name: &str,
        return_type: EvalParameterType,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| {
                function.set_return_type(return_type);
                true
            })
    }

    /// Records whether eval may dispatch a native AOT callback through its bridge.
    pub fn define_native_function_bridge_supported(
        &mut self,
        function_name: &str,
        supported: bool,
    ) -> bool {
        self.native_functions
            .get_mut(function_name)
            .is_some_and(|function| {
                function.set_bridge_supported(supported);
                true
            })
    }
}
