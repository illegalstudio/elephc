//! Purpose:
//! Declares the opaque process-level eval context handle.
//! The full implementation will hold dynamic function, class, constant, and
//! builtin registries plus runtime hooks.
//!
//! Called from:
//! - `crate::abi`
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - The handle is intentionally opaque to generated code.
//! - No Rust-owned layout is promised across the C ABI.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use crate::abi::ABI_VERSION;
use crate::eval_ir::EvalFunction;
use crate::scope::ElephcEvalScope;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Native descriptor-invoker ABI registered by generated code for AOT functions.
pub type NativeFunctionInvoker =
    unsafe extern "C" fn(*mut c_void, *mut RuntimeCell) -> *mut RuntimeCell;

/// Native AOT function callback metadata visible to runtime eval fragments.
#[derive(Clone)]
pub struct NativeFunction {
    descriptor: *mut c_void,
    invoker: NativeFunctionInvoker,
    param_count: usize,
    param_names: Vec<String>,
}

impl NativeFunction {
    /// Creates callback metadata for a descriptor-compatible AOT function.
    pub fn new(
        descriptor: *mut c_void,
        invoker: NativeFunctionInvoker,
        param_count: usize,
    ) -> Self {
        Self {
            descriptor,
            invoker,
            param_count,
            param_names: Vec::new(),
        }
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

    /// Invokes the descriptor-compatible callback with a boxed Mixed arg array.
    ///
    /// # Safety
    /// `arg_array` must be a boxed Mixed indexed array whose elements are boxed
    /// Mixed cells following the descriptor-invoker ABI.
    pub unsafe fn call(&self, arg_array: RuntimeCellHandle) -> RuntimeCellHandle {
        RuntimeCellHandle::from_raw((self.invoker)(self.descriptor, arg_array.as_ptr()))
    }
}

/// Process-level eval context passed opaquely across the C ABI.
///
/// Generated code never inspects this layout directly; it only passes pointers
/// back to the eval bridge. Keeping a concrete Rust type here lets the bridge
/// grow dynamic registries without exposing them to generated assembly.
pub struct ElephcEvalContext {
    abi_version: u32,
    classes: HashSet<String>,
    constants: HashMap<String, RuntimeCellHandle>,
    functions: HashMap<String, EvalFunction>,
    native_functions: HashMap<String, NativeFunction>,
    static_locals: HashMap<(String, String), RuntimeCellHandle>,
    included_files: HashSet<String>,
    global_scope: Option<*mut ElephcEvalScope>,
    function_stack: Vec<String>,
    pending_throw: Option<RuntimeCellHandle>,
    call_file: String,
    call_dir: String,
    call_line: i64,
    file_magic_override: Option<String>,
}

impl ElephcEvalContext {
    /// Creates a context using the current eval bridge ABI version.
    pub fn new() -> Self {
        Self {
            abi_version: ABI_VERSION,
            classes: HashSet::new(),
            constants: HashMap::new(),
            functions: HashMap::new(),
            native_functions: HashMap::new(),
            static_locals: HashMap::new(),
            included_files: HashSet::new(),
            global_scope: None,
            function_stack: Vec::new(),
            pending_throw: None,
            call_file: String::new(),
            call_dir: String::new(),
            call_line: 0,
            file_magic_override: None,
        }
    }

    /// Creates a context with an explicit ABI version for compatibility tests.
    #[cfg(test)]
    pub fn for_abi_version(abi_version: u32) -> Self {
        Self {
            abi_version,
            classes: HashSet::new(),
            constants: HashMap::new(),
            functions: HashMap::new(),
            native_functions: HashMap::new(),
            static_locals: HashMap::new(),
            included_files: HashSet::new(),
            global_scope: None,
            function_stack: Vec::new(),
            pending_throw: None,
            call_file: String::new(),
            call_dir: String::new(),
            call_line: 0,
            file_magic_override: None,
        }
    }

    /// Returns the ABI version this context was created for.
    pub const fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Defines an eval-declared class name, failing if this context already has it.
    pub fn define_class(&mut self, name: &str) -> bool {
        let key = normalize_class_name(name);
        if self.classes.contains(&key) {
            return false;
        }
        self.classes.insert(key);
        true
    }

    /// Returns true when this eval context has a dynamic class with the requested name.
    pub fn has_class(&self, name: &str) -> bool {
        self.classes.contains(&normalize_class_name(name))
    }

    /// Defines an eval dynamic constant value, failing if the name is invalid or already present.
    pub fn define_constant(&mut self, name: &str, value: RuntimeCellHandle) -> bool {
        let key = normalize_constant_name(name);
        if key.is_empty() || self.constants.contains_key(&key) {
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

    /// Returns true when the context has a dynamic or native function with this lowercase PHP name.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name) || self.native_functions.contains_key(name)
    }

    /// Returns a stored static local cell for an eval-declared function.
    pub fn static_local(&self, function_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_locals
            .get(&(function_name.to_string(), name.to_string()))
            .copied()
    }

    /// Stores one static local cell and returns any replaced distinct cell.
    pub fn set_static_local(
        &mut self,
        function_name: impl Into<String>,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .static_locals
            .insert((function_name.into(), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns true when an eval include key was already loaded by this context.
    pub fn has_included_file(&self, path: &str) -> bool {
        self.included_files.contains(path)
    }

    /// Records one successfully loaded eval include key for include_once/require_once.
    pub fn mark_included_file(&mut self, path: impl Into<String>) {
        self.included_files.insert(path.into());
    }

    /// Stores the non-owned global scope handle used by eval `global` aliases.
    pub fn set_global_scope(&mut self, scope: *mut ElephcEvalScope) -> bool {
        if scope.is_null() {
            self.global_scope = None;
            false
        } else {
            self.global_scope = Some(scope);
            true
        }
    }

    /// Returns the non-owned global scope handle for eval `global` aliases.
    pub fn global_scope_ptr(&self) -> Option<*mut ElephcEvalScope> {
        self.global_scope
    }

    /// Pushes an eval-executed function name for magic-constant resolution.
    pub fn push_function(&mut self, name: impl Into<String>) {
        self.function_stack.push(name.into());
    }

    /// Pops the current eval-executed function name after its body completes.
    pub fn pop_function(&mut self) {
        self.function_stack.pop();
    }

    /// Returns the current eval-executed function name, if execution is inside one.
    pub fn current_function(&self) -> Option<&str> {
        self.function_stack.last().map(String::as_str)
    }

    /// Records a Throwable cell that escaped from an eval-executed function call.
    pub fn set_pending_throw(&mut self, value: RuntimeCellHandle) {
        self.pending_throw = Some(value);
    }

    /// Returns and clears the Throwable cell currently escaping through eval.
    pub fn take_pending_throw(&mut self) -> Option<RuntimeCellHandle> {
        self.pending_throw.take()
    }

    /// Updates the source file, directory, and line for the current eval call site.
    pub fn set_call_site(&mut self, file: impl Into<String>, dir: impl Into<String>, line: i64) {
        self.call_file = file.into();
        self.call_dir = dir.into();
        self.call_line = line;
        self.file_magic_override = None;
    }

    /// Returns a copy of the current call-site metadata for temporary overrides.
    pub fn call_site(&self) -> (String, String, i64, Option<String>) {
        (
            self.call_file.clone(),
            self.call_dir.clone(),
            self.call_line,
            self.file_magic_override.clone(),
        )
    }

    /// Overrides `__FILE__` while executing an actual file through eval include.
    pub fn set_file_magic_override(&mut self, file: Option<String>) {
        self.file_magic_override = file;
    }

    /// Returns the source directory associated with the current eval call site.
    pub fn call_dir(&self) -> &str {
        &self.call_dir
    }

    /// Returns PHP's `__FILE__` string for code currently running inside eval.
    pub fn eval_file_magic(&self) -> String {
        if let Some(file) = &self.file_magic_override {
            return file.clone();
        }
        if self.call_file.is_empty() {
            return String::new();
        }
        format!("{}({}) : eval()'d code", self.call_file, self.call_line)
    }
}

impl Default for ElephcEvalContext {
    /// Creates the default process-level eval context.
    fn default() -> Self {
        Self::new()
    }
}

/// Normalizes PHP class names for the eval dynamic class registry.
fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Normalizes PHP constant names for case-sensitive eval dynamic probes.
fn normalize_constant_name(name: &str) -> String {
    name.trim_start_matches('\\').to_string()
}
