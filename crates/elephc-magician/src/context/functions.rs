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
    Resource(i64),
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

    /// Defines an eval dynamic constant value, failing if the name is invalid or already present.
    pub fn define_constant(&mut self, name: &str, value: RuntimeCellHandle) -> bool {
        let key = normalize_constant_name(name);
        if key.is_empty()
            || self.constants.contains_key(&key)
            || self.native_global_constants.contains_key(&key)
        {
            return false;
        }
        self.constants.insert(key, value);
        true
    }

    /// Returns true when this eval context has a dynamic constant with the requested name.
    pub fn has_constant(&self, name: &str) -> bool {
        self.constants.contains_key(&normalize_constant_name(name))
    }

    /// Returns an eval dynamic constant value by case-sensitive PHP constant name.
    pub fn constant(&self, name: &str) -> Option<RuntimeCellHandle> {
        self.constants.get(&normalize_constant_name(name)).copied()
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

    /// Returns eval and generated native user-function names in stable order.
    pub(crate) fn defined_user_function_names(&self) -> Vec<String> {
        let mut names = self.functions.keys().cloned().collect::<Vec<_>>();
        names.extend(self.native_functions.keys().cloned());
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
        if self.functions.contains_key(&name) || self.native_functions.contains_key(&name) {
            return Err(function);
        }
        self.functions.insert(name, function);
        Ok(())
    }

    /// Stores one eval closure instance under a context-local synthetic callable name.
    pub fn define_closure(&mut self, closure: EvalClosure) -> String {
        let name = format!("{{closure:eval:{}}}", self.next_closure_id);
        self.next_closure_id += 1;
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
        self.closure_objects.insert(identity, target);
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

    /// Defines a generated native function callback, failing if the name already exists.
    pub fn define_native_function(
        &mut self,
        name: impl Into<String>,
        function: NativeFunction,
    ) -> Result<(), NativeFunction> {
        let name = name.into();
        if self.functions.contains_key(&name) || self.native_functions.contains_key(&name) {
            return Err(function);
        }
        self.native_functions.insert(name, function);
        Ok(())
    }

    /// Returns a dynamic user function by its lowercase PHP function name.
    pub fn function(&self, name: &str) -> Option<&EvalFunction> {
        self.functions.get(name)
    }

    /// Returns a dynamic eval closure by its synthetic callable name.
    pub fn closure(&self, name: &str) -> Option<&EvalClosure> {
        self.closures.get(name)
    }

    /// Returns a native AOT function callback by its lowercase PHP function name.
    pub fn native_function(&self, name: &str) -> Option<NativeFunction> {
        self.native_functions.get(name).cloned()
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
