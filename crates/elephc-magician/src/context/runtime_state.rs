//! Purpose:
//! Manages mutable eval runtime state, execution scopes, resources, and call-site metadata.
//!
//! Called from:
//! - Interpreter execution and builtins requiring process-level eval state.
//!
//! Key details:
//! - Static cells, include state, scope stacks, errors, timezone, HTTP status, and magic paths live here.

use super::*;

impl ElephcEvalContext {
    /// Returns true when the context has a dynamic or native function with this lowercase PHP name.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name) || self.native_functions.contains_key(name)
    }

    /// Returns true when the context has a closure registered under this synthetic name.
    pub fn has_closure(&self, name: &str) -> bool {
        self.closures.contains_key(name)
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

    /// Returns a stored static property cell for an eval-declared class.
    pub fn static_property(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_properties
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
    }

    /// Stores one eval static property cell and returns any replaced distinct cell.
    pub fn set_static_property(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .static_properties
            .insert((normalize_class_name(class_name), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Binds one eval static property slot to a persistent PHP reference target.
    pub fn bind_static_property_alias(
        &mut self,
        class_name: &str,
        name: &str,
        target: EvalReferenceTarget,
    ) -> Option<EvalReferenceTarget> {
        self.static_property_aliases
            .insert((normalize_class_name(class_name), name.to_string()), target)
    }

    /// Returns the persistent reference target bound to one eval static property slot.
    pub fn static_property_alias(
        &self,
        class_name: &str,
        name: &str,
    ) -> Option<&EvalReferenceTarget> {
        self.static_property_aliases
            .get(&(normalize_class_name(class_name), name.to_string()))
    }

    /// Returns a materialized eval class constant cell.
    pub fn class_constant_cell(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.class_constants
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
    }

    /// Stores one eval class constant cell and returns any replaced distinct cell.
    pub fn set_class_constant_cell(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .class_constants
            .insert((normalize_class_name(class_name), name.into()), cell);
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

    /// Pushes the eval class whose method is currently executing.
    pub fn push_class_scope(&mut self, name: impl Into<String>) {
        self.class_stack.push(name.into());
    }

    /// Pops the current eval class method scope.
    pub fn pop_class_scope(&mut self) {
        self.class_stack.pop();
    }

    /// Returns the current eval class scope, if execution is inside a method.
    pub fn current_class_scope(&self) -> Option<&str> {
        self.class_stack.last().map(String::as_str)
    }

    /// Pushes the class name used to dispatch the current eval method call.
    pub fn push_called_class_scope(&mut self, name: impl Into<String>) {
        self.called_class_stack.push(name.into());
    }

    /// Pops the current late-static-bound eval class scope.
    pub fn pop_called_class_scope(&mut self) {
        self.called_class_stack.pop();
    }

    /// Returns the current late-static-bound eval class scope, if execution is inside a method.
    pub fn current_called_class_scope(&self) -> Option<&str> {
        self.called_class_stack.last().map(String::as_str)
    }

    /// Returns a dynamic called-class override for a generated/AOT frame entering eval.
    pub fn native_frame_called_class_override(
        &self,
        class_name: &str,
        called_class_name: &str,
    ) -> Option<String> {
        let class_name = class_name.trim_start_matches('\\');
        let called_class_name = called_class_name.trim_start_matches('\\');
        if class_name.is_empty() || !called_class_name.eq_ignore_ascii_case(class_name) {
            return None;
        }
        if let Some(called_class) =
            native_frame_called_class_override(class_name, called_class_name)
        {
            return Some(called_class);
        }
        let active = self.current_called_class_scope()?.trim_start_matches('\\');
        if active.is_empty() || active.eq_ignore_ascii_case(class_name) {
            return None;
        }
        let active = self
            .resolve_class_name(active)
            .unwrap_or_else(|| active.to_string());
        self.class_parent_names(&active)
            .iter()
            .any(|parent| parent.eq_ignore_ascii_case(class_name))
            .then_some(active)
    }

    /// Pushes PHP-visible method magic constants for the current eval method frame.
    pub fn push_method_magic_scope(&mut self, class_name: &str, method: &EvalClassMethod) {
        self.magic_stack.push(EvalMagicScope {
            function_name: method.magic_function_name().to_string(),
            method_name: method.magic_method_name(class_name),
            class_name: class_name.trim_start_matches('\\').to_string(),
            trait_name: method
                .trait_origin()
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
        });
    }

    /// Pushes PHP-visible class-like member magic constants for default expressions.
    pub fn push_class_like_member_magic_scope(
        &mut self,
        class_name: &str,
        trait_name: Option<&str>,
    ) {
        self.magic_stack.push(EvalMagicScope {
            function_name: String::new(),
            method_name: String::new(),
            class_name: class_name.trim_start_matches('\\').to_string(),
            trait_name: trait_name
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
        });
    }

    /// Pushes PHP-visible callable magic constants for reflected parameter defaults.
    pub fn push_callable_magic_scope(
        &mut self,
        function_name: &str,
        method_name: &str,
        class_name: Option<&str>,
        trait_name: Option<&str>,
    ) {
        self.magic_stack.push(EvalMagicScope {
            function_name: function_name.to_string(),
            method_name: method_name.to_string(),
            class_name: class_name
                .map(|class_name| class_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
            trait_name: trait_name
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
        });
    }

    /// Pops the current PHP-visible eval magic-constant scope.
    pub fn pop_magic_scope(&mut self) {
        self.magic_stack.pop();
    }

    /// Returns the PHP `__FUNCTION__` value for the current eval frame.
    pub fn current_magic_function(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.function_name.as_str())
    }

    /// Returns the PHP `__METHOD__` value for the current eval method frame.
    pub fn current_magic_method(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.method_name.as_str())
    }

    /// Returns the PHP `__CLASS__` value for the current eval method frame.
    pub fn current_magic_class(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.class_name.as_str())
    }

    /// Returns the PHP `__TRAIT__` value for the current eval method frame.
    pub fn current_magic_trait(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.trait_name.as_str())
    }

    /// Captures the current eval execution stacks for later caller-context-sensitive work.
    pub fn execution_scope(&self) -> ElephcEvalExecutionScope {
        ElephcEvalExecutionScope {
            function_stack: self.function_stack.clone(),
            class_stack: self.class_stack.clone(),
            called_class_stack: self.called_class_stack.clone(),
        }
    }

    /// Replaces eval execution stacks and returns the previous stacks for restoration.
    pub fn replace_execution_scope(
        &mut self,
        scope: ElephcEvalExecutionScope,
    ) -> ElephcEvalExecutionScope {
        ElephcEvalExecutionScope {
            function_stack: std::mem::replace(&mut self.function_stack, scope.function_stack),
            class_stack: std::mem::replace(&mut self.class_stack, scope.class_stack),
            called_class_stack: std::mem::replace(
                &mut self.called_class_stack,
                scope.called_class_stack,
            ),
        }
    }

    /// Records a Throwable cell that escaped from an eval-executed function call.
    pub fn set_pending_throw(&mut self, value: RuntimeCellHandle) {
        self.pending_throw = Some(value);
    }

    /// Returns and clears the Throwable cell currently escaping through eval.
    pub fn take_pending_throw(&mut self) -> Option<RuntimeCellHandle> {
        self.pending_throw.take()
    }

    /// Returns the eval-local SPL autoload extension list.
    pub fn spl_autoload_extensions(&self) -> &str {
        &self.spl_autoload_extensions
    }

    /// Replaces the eval-local SPL autoload extension list.
    pub fn set_spl_autoload_extensions(&mut self, extensions: impl Into<String>) {
        self.spl_autoload_extensions = extensions.into();
    }

    /// Returns the eval-local stream resource table.
    pub(crate) fn stream_resources(&self) -> &EvalStreamResources {
        &self.streams
    }

    /// Returns mutable access to the eval-local stream resource table.
    pub(crate) fn stream_resources_mut(&mut self) -> &mut EvalStreamResources {
        &mut self.streams
    }

    /// Clears the eval-local JSON error state after a successful JSON operation.
    pub fn clear_json_error(&mut self) {
        self.json_last_error = 0;
        self.json_last_error_msg.clear();
        self.json_last_error_msg.push_str("No error");
    }

    /// Records the eval-local JSON error state for `json_last_error*()` calls.
    pub fn set_json_error(&mut self, code: i64, message: impl Into<String>) {
        self.json_last_error = code;
        self.json_last_error_msg = message.into();
    }

    /// Returns the PHP `JSON_ERROR_*` code for the last eval JSON operation.
    pub const fn json_last_error(&self) -> i64 {
        self.json_last_error
    }

    /// Returns the PHP message for the last eval JSON operation.
    pub fn json_last_error_msg(&self) -> &str {
        &self.json_last_error_msg
    }

    /// Returns the eval-local PHP default timezone identifier.
    pub fn default_timezone(&self) -> &str {
        &self.default_timezone
    }

    /// Replaces the eval-local PHP default timezone identifier.
    pub fn set_default_timezone(&mut self, timezone: impl Into<String>) {
        self.default_timezone = timezone.into();
    }

    /// Returns the eval-local HTTP response code used by web-facing builtins.
    pub const fn http_response_code(&self) -> i64 {
        self.http_response_code
    }

    /// Applies a new eval-local HTTP response code and returns the previous one.
    pub fn replace_http_response_code(&mut self, response_code: i64) -> i64 {
        let previous = self.http_response_code;
        if response_code > 0 {
            self.http_response_code = response_code;
        }
        previous
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
