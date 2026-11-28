//! Purpose:
//! Manages mutable eval runtime state, execution scopes, resources, and call-site metadata.
//!
//! Called from:
//! - Interpreter execution and builtins requiring process-level eval state.
//!
//! Key details:
//! - Static cells, include state, scope stacks, errors, timezone, HTTP status, and magic paths live here.
//! - Internal array pointers live here too, because runtime array cells carry no
//!   PHP-visible cursor of their own.

use super::*;
use crate::errors::EvalStatus;
use crate::interpreter::RuntimeValueOps;

impl ElephcEvalContext {
    /// Returns whether an earlier complex fragment requires full scope synchronization.
    pub(crate) const fn eval_scope_sync_requires_full(&self) -> bool {
        self.scope_sync_requires_full
    }

    /// Permanently selects full scope synchronization for this activation context.
    pub(crate) fn require_full_eval_scope_sync(&mut self) {
        self.scope_sync_requires_full = true;
    }

    /// Requires full escape validation once foreign callable metadata is possible.
    /// Draining cell pins or replacing a handler never clears this capability.
    pub(crate) fn needs_pcntl_escape_scan(&self) -> bool {
        self.may_have_foreign_pcntl_callables
    }

    /// Pins one foreign callable cell and the context metadata needed to invoke it.
    pub(crate) fn retain_pcntl_foreign_callable(
        &mut self,
        callback: RuntimeCellHandle,
        owner: pcntl_runtime::EvalPcntlContextLease,
        values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let identity = callback.as_ptr() as usize;
        if self.pcntl_foreign_callables.contains_key(&identity) {
            return Ok(());
        }
        let retained = values.retain(callback)?;
        self.pcntl_foreign_callables
            .insert(identity, retained, owner);
        self.may_have_foreign_pcntl_callables = true;
        Ok(())
    }

    /// Propagates foreign callable metadata to an independently copied PHP value.
    pub(crate) fn copy_pcntl_foreign_callable(
        &mut self,
        source: RuntimeCellHandle,
        target: RuntimeCellHandle,
        values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let source = source.as_ptr() as usize;
        let target_identity = target.as_ptr() as usize;
        if source == target_identity || self.pcntl_foreign_callables.contains_key(&target_identity) {
            return Ok(());
        }
        let Some(owner) = self
            .pcntl_foreign_callables
            .metadata(&source)
            .cloned()
        else {
            return Ok(());
        };
        let retained = values.retain(target)?;
        self.pcntl_foreign_callables
            .insert(target_identity, retained, owner);
        self.may_have_foreign_pcntl_callables = true;
        Ok(())
    }

    /// Returns the retained owner lease for a PCNTL callable exported into this context.
    pub(crate) fn pcntl_foreign_callable_owner(
        &self,
        callback: RuntimeCellHandle,
    ) -> Option<&pcntl_runtime::EvalPcntlContextLease> {
        self.pcntl_foreign_callables
            .metadata(&(callback.as_ptr() as usize))
    }

    /// Retires each exported cell before its context lease, chaining both teardown results.
    pub(crate) fn release_pcntl_foreign_callables(
        &mut self,
        mut pending: Option<RuntimeCellHandle>,
        mut release: impl FnMut(RuntimeCellHandle, Option<RuntimeCellHandle>) -> Option<RuntimeCellHandle>,
    ) -> Option<RuntimeCellHandle> {
        for (callback, owner) in self.pcntl_foreign_callables.take_all() {
            pending = owner.retire(release(callback, pending));
        }
        pending
    }

    /// Returns true when the context has a dynamic or native function with this lowercase PHP name.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name) || self.native_functions.contains_key(name)
    }

    /// Returns true when the context has a closure registered under this synthetic name.
    pub fn has_closure(&self, name: &str) -> bool {
        self.closures.contains_key(name)
    }

    /// Reports whether losing a PCNTL callback would destroy request-persistent declarations.
    pub(crate) fn has_pcntl_request_cells(&self) -> bool {
        !self.static_locals.is_empty()
            || !self.static_properties.is_empty()
            || !self.constants.is_empty()
            || !self.class_constants.is_empty()
            || !self.enum_cases.is_empty()
            || !self.enum_case_values.is_empty()
            || !self.instance_defaults.is_empty()
    }

    /// Reports whether terminal ownership convergence still has a context-owned PHP cell.
    pub(crate) fn has_context_cell_owners(&self) -> bool {
        self.has_pcntl_request_cells()
            || !self.error_handlers.is_empty()
            || !self.exception_handlers.is_empty()
            || self.streams.has_php_cell_owners()
    }

    /// Detaches persistent value owners before native releases can reenter the context.
    pub(crate) fn take_context_cell_owners(
        &mut self,
    ) -> elephc_memoir::runtime_owners::RuntimeOwnerBatch {
        let mut owners = self.enum_cases.take_all();
        owners.extend(self.enum_case_values.take_all());
        owners.extend(self.constants.take_all());
        owners.extend(self.static_locals.take_all());
        owners.extend(self.static_properties.take_all());
        owners.extend(self.class_constants.take_all());
        owners.extend(self.instance_defaults.take_all());
        owners.extend(
            self.error_handlers
                .take_all()
                .into_iter()
                .map(|handler| handler.callback),
        );
        owners.extend(self.exception_handlers.take_all());
        owners
    }

    /// Prevents nested ABI owner releases from freeing a context during destructor callbacks.
    pub(crate) fn begin_teardown(&mut self) -> bool {
        if self.teardown_active || self.has_abi_owners() { return false; }
        self.teardown_active = true;
        true
    }

    /// Ends the callback-sensitive release pass before final context reclamation.
    pub(crate) fn end_teardown(&mut self) { self.teardown_active = false; }

    /// Reports whether a native release callback is still using this context.
    pub(crate) fn teardown_active(&self) -> bool { self.teardown_active }

    /// Returns a stored static local cell for an eval-declared function.
    pub fn static_local(&self, function_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_locals
            .get(&(function_name.to_string(), name.to_string()))
            .copied()
            .map(RuntimeCellHandle::borrowed)
    }

    /// Stores one static local cell and returns any replaced distinct cell.
    pub fn set_static_local(
        &mut self,
        function_name: impl Into<String>,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.static_locals
            .replace_distinct((function_name.into(), name.into()), cell)
    }

    /// Returns a stored static property cell for an eval-declared class.
    pub fn static_property(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_properties
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
            .map(RuntimeCellHandle::borrowed)
    }

    /// Stores one eval static property cell and returns any replaced distinct cell.
    pub fn set_static_property(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.static_properties
            .replace_distinct((normalize_class_name(class_name), name.into()), cell)
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
            .map(RuntimeCellHandle::borrowed)
    }

    /// Returns the instance default `class_name` evaluated for property storage `name` when it
    /// was initialized.
    pub fn instance_default_cell(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.instance_defaults
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
            .map(RuntimeCellHandle::borrowed)
    }

    /// Stores one evaluated instance default and returns any replaced distinct cell.
    pub fn set_instance_default_cell(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.instance_defaults
            .replace_distinct((normalize_class_name(class_name), name.into()), cell)
    }

    /// Stores one eval class constant cell and returns any replaced distinct cell.
    pub fn set_class_constant_cell(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        self.class_constants
            .replace_distinct((normalize_class_name(class_name), name.into()), cell)
    }

    /// Returns the PHP internal array pointer tracked for one runtime array cell.
    ///
    /// Cells without a stored cursor answer `Position(0)`, matching PHP, where a
    /// freshly built array points at its first element.
    pub fn array_cursor(&self, array: RuntimeCellHandle) -> EvalArrayCursor {
        self.array_cursors
            .get(array.as_ptr() as usize)
            .unwrap_or(EvalArrayCursor::Position(0))
    }

    /// Stores the PHP internal array pointer for one runtime array cell.
    ///
    /// The default cursor is dropped instead of stored so a later array cell that
    /// reuses this address starts from PHP's fresh-array state.
    pub fn set_array_cursor(&mut self, array: RuntimeCellHandle, cursor: EvalArrayCursor) {
        let key = array.as_ptr() as usize;
        if cursor == EvalArrayCursor::Position(0) {
            let _ = self.array_cursors.remove(key);
            return;
        }
        let _ = self.array_cursors.insert(key, cursor);
    }

    /// Returns true when an eval include key was already loaded by this context.
    pub fn has_included_file(&self, path: &str) -> bool {
        self.included_files.contains(path)
    }

    /// Records one successfully loaded eval include key for include_once/require_once.
    ///
    /// The set retains the existing once-deduplication behavior; the separate list
    /// records first inclusion rather than reconstructing a lexical order on reads.
    /// The main script may already have been seeded without joining the once set.
    pub fn mark_included_file(&mut self, path: impl Into<String>) {
        let path = path.into();
        if self.included_files.insert(path.clone())
            && self.included_main_file.as_deref() != Some(path.as_str())
        {
            self.included_file_order.push(path);
        }
    }

    /// Seeds the first nonempty call site once, independently of earlier include entries.
    ///
    /// A nested include temporarily changes `call_file`, so subsequent call sites
    /// must never reorder the inventory. Keep the seed out of the once set to
    /// preserve that set's existing behavior, including explicitly including main.
    fn seed_main_included_file(&mut self) {
        if self.call_file.is_empty() || self.included_main_file.is_some() {
            return;
        }
        if let Some(position) = self.included_file_order.iter().position(|name| name == &self.call_file) {
            self.included_file_order.remove(position);
        }
        self.included_file_order.insert(0, self.call_file.clone());
        self.included_main_file = Some(self.call_file.clone());
    }

    /// Returns the stable main entry followed by files in first-inclusion order.
    pub(crate) fn included_file_names(&self) -> Vec<String> {
        self.included_file_order.clone()
    }

    /// Returns the current error reporting mask, replacing it when a new mask is supplied.
    pub(crate) fn update_error_reporting(&mut self, replacement: Option<i64>) -> i64 {
        let previous = self.error_reporting;
        if let Some(replacement) = replacement {
            self.error_reporting = replacement;
        }
        previous
    }

    /// Returns the current error reporting mask without mutating it.
    pub(crate) const fn error_reporting_mask(&self) -> i64 {
        self.error_reporting
    }

    /// Installs an eval user error handler and returns the previously active handler.
    pub(crate) fn push_error_handler(
        &mut self,
        replacement: Option<EvalErrorHandlerState>,
    ) -> Option<EvalErrorHandlerState> {
        self.error_handlers.push(replacement)
    }

    /// Restores the previous eval user error handler and returns the discarded current handler.
    pub(crate) fn restore_error_handler_state(
        &mut self,
    ) -> Option<EvalErrorHandlerState> {
        self.error_handlers.restore()
    }

    /// Returns the currently active eval user error handler.
    pub(crate) const fn error_handler_state(&self) -> Option<EvalErrorHandlerState> {
        self.error_handlers.current()
    }

    /// Temporarily removes the active handler without changing its restoration stack.
    pub(crate) fn suspend_error_handler(&mut self) -> Option<EvalErrorHandlerState> {
        self.error_handlers.suspend()
    }

    /// Restores a suspended handler unless its callback installed a replacement.
    pub(crate) fn resume_error_handler(
        &mut self,
        suspended: EvalErrorHandlerState,
    ) -> Option<EvalErrorHandlerState> {
        self.error_handlers.resume(suspended)
    }

    /// Installs an eval uncaught-exception handler and returns the previous handler.
    pub(crate) fn push_exception_handler(
        &mut self,
        replacement: Option<RuntimeCellHandle>,
    ) -> Option<RuntimeCellHandle> {
        self.exception_handlers.push(replacement)
    }

    /// Restores the previous eval exception handler and returns the discarded handler.
    pub(crate) fn restore_exception_handler_state(&mut self) -> Option<RuntimeCellHandle> {
        self.exception_handlers.restore()
    }

    /// Returns the currently active eval uncaught-exception handler without transferring ownership.
    pub(crate) const fn exception_handler_state(&self) -> Option<RuntimeCellHandle> {
        self.exception_handlers.current()
    }

    /// Returns active eval call frames from the current frame to the outermost frame.
    pub(crate) fn backtrace_frames(&self) -> Vec<EvalBacktraceFrame> {
        let mut frames = Vec::new();
        for depth in 0..=self.function_args_stack.len() {
            frames.extend(self.eval_backtrace_boundaries.iter()
                .filter(|(position, _)| *position == depth)
                .map(|(_, frame)| frame.clone()));
            if let Some(frame) = self.function_args_stack.get(depth) {
                frames.push(frame.clone());
            }
        }
        frames.reverse();
        frames
    }

    /// Records an eval boundary without changing func_* argument introspection scope.
    pub(crate) fn push_eval_backtrace_boundary(&mut self) {
        let (file, _, line, _) = self.call_site();
        let frame = EvalBacktraceFrame::new("eval".to_string(),
            EvalFunctionArgsFrame::new(Vec::new(), 0, Vec::new()), file, line,
            None, None, None);
        self.eval_backtrace_boundaries.push((self.function_args_stack.len(), frame));
    }

    /// Removes an eval boundary after successful execution or an interpreted throwable.
    pub(crate) fn pop_eval_backtrace_boundary(&mut self) {
        self.eval_backtrace_boundaries.pop();
    }

    /// Reports whether an `eval()` call (bridge or nested) is executing in this context.
    ///
    /// The native frame that installed the global scope handle is alive for the whole call,
    /// whereas a context a callback retains can outlive that frame and its scopes.
    pub(crate) fn in_eval_execution(&self) -> bool {
        !self.eval_backtrace_boundaries.is_empty()
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

    /// Pushes PHP argument-introspection metadata for a free-function activation.
    pub(crate) fn push_function_args(&mut self, frame: EvalFunctionArgsFrame) {
        self.push_function_args_with_backtrace(frame, None, None, false);
    }

    /// Pushes PHP argument metadata together with method or bound-closure backtrace metadata.
    pub(crate) fn push_function_args_with_backtrace(
        &mut self,
        frame: EvalFunctionArgsFrame,
        class_name: Option<String>,
        object: Option<RuntimeCellHandle>,
        is_static: bool,
    ) {
        let function = self
            .current_function()
            .unwrap_or_default()
            .trim_start_matches('\\')
            .to_string();
        let function = class_name
            .as_ref()
            .and_then(|_| function.rsplit_once("::").map(|(_, method)| method.to_string()))
            .unwrap_or(function);
        let (file, _, line, _) = self.call_site();
        let call_type = class_name
            .as_ref()
            .map(|_| if is_static { "::" } else { "->" });
        self.function_args_stack.push(EvalBacktraceFrame::new(
            function,
            frame,
            file,
            line,
            class_name,
            object,
            call_type,
        ));
    }

    /// Pops activation metadata and transfers its retained snapshot cells for release.
    pub(crate) fn pop_function_args(
        &mut self,
    ) -> elephc_memoir::runtime_owners::RuntimeOwnerBatch {
        self.function_args_stack.pop()
            .map(EvalBacktraceFrame::into_argument_cells).unwrap_or_default()
    }

    /// Returns PHP argument-introspection metadata for the active callable.
    pub(crate) fn current_function_args(&self) -> Option<&EvalFunctionArgsFrame> {
        self.function_args_stack
            .last()
            .map(EvalBacktraceFrame::arguments)
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
        self.class_stack
            .last()
            .and_then(|name| (!name.is_empty()).then_some(name.as_str()))
    }

    /// Starts one readonly reinitialization phase for a newly cloned object.
    pub(crate) fn begin_clone_initialization(&mut self, identity: u64) {
        self.clone_reinitialized_properties
            .insert(identity, HashSet::new());
    }

    /// Starts the post-hook readonly reinitialization phase for clone property overrides.
    pub(crate) fn restart_clone_initialization(&mut self, identity: u64) {
        if let Some(properties) = self.clone_reinitialized_properties.get_mut(&identity) {
            properties.clear();
        }
    }

    /// Ends readonly reinitialization for one cloned object.
    pub(crate) fn end_clone_initialization(&mut self, identity: u64) {
        self.clone_reinitialized_properties.remove(&identity);
    }

    /// Returns whether the object is inside either readonly clone reinitialization phase.
    pub(crate) fn clone_initialization_is_active(&self, identity: u64) -> bool {
        self.clone_reinitialized_properties.contains_key(&identity)
    }

    /// Checks a readonly clone allowance without consuming it before visibility validation.
    pub(crate) fn clone_property_can_be_reinitialized(&self, identity: u64, property: &str) -> bool {
        self.clone_reinitialized_properties
            .get(&identity)
            .is_some_and(|properties| !properties.contains(property))
    }

    /// Consumes one property's single readonly rewrite allowance in the active phase.
    pub(crate) fn consume_clone_reinitialization(
        &mut self,
        identity: u64,
        property: &str,
    ) -> bool {
        self.clone_reinitialized_properties
            .get_mut(&identity)
            .is_some_and(|properties| properties.insert(property.to_string()))
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
        self.called_class_stack
            .last()
            .and_then(|name| (!name.is_empty()).then_some(name.as_str()))
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
        let previous = self.pending_throw.replace(Some(value));
        debug_assert!(previous.is_none(), "pending Throwable replaced before it was taken");
    }

    /// Returns and clears the Throwable cell currently escaping through eval.
    pub fn take_pending_throw(&mut self) -> Option<RuntimeCellHandle> {
        self.pending_throw.take_optional()
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
    ///
    /// The first nonempty site seeds main once; temporary include sites and their
    /// restoration never change the order subsequently reported by file inventories.
    pub fn set_call_site(&mut self, file: impl Into<String>, dir: impl Into<String>, line: i64) {
        self.call_file = file.into();
        self.call_dir = dir.into();
        self.call_line = line;
        self.file_magic_override = None;
        self.seed_main_included_file();
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

#[cfg(test)]
mod persistent_owner_tests {
    use super::*;

    /// Every map entry owns a lease, including equal handles stored in different maps.
    #[test]
    fn persistent_cell_drain_preserves_each_map_lease() {
        let mut context = ElephcEvalContext::new();
        let first = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        let second = RuntimeCellHandle::from_raw(2usize as *mut crate::value::RuntimeCell);
        context.set_static_local("function", "local", first);
        context.set_static_property("Owner", "value", first);
        context.set_class_constant_cell("Owner", "TOKEN", second);
        let cells = context.take_context_cell_owners();
        assert_eq!(cells.iter().filter(|cell| *cell == first).count(), 2);
        assert_eq!(cells.iter().filter(|cell| *cell == second).count(), 1);
        assert!(context.static_local("function", "local").is_none());
        assert!(context.static_property("Owner", "value").is_none());
        assert!(context.class_constant_cell("Owner", "TOKEN").is_none());
        assert!(context.take_context_cell_owners().is_empty());
        context.set_static_property("Owner", "replacement", second);
        assert_eq!(
            context
                .take_context_cell_owners()
                .iter()
                .collect::<Vec<_>>(),
            vec![second],
        );
    }

    /// Dynamic declarations and local handler stacks participate in the same owner drain.
    #[test]
    fn context_cell_drain_includes_constants_enums_and_handlers() {
        let mut context = ElephcEvalContext::new();
        let cells = (1..=7)
            .map(|address| RuntimeCellHandle::from_raw(address as *mut crate::value::RuntimeCell))
            .collect::<Vec<_>>();
        context.constants.insert("dynamic".to_string(), cells[0]);
        context.enum_cases.insert(("State".to_string(), "Open".to_string()), cells[1]);
        context.enum_case_values.insert(("State".to_string(), "Open".to_string()), cells[2]);
        context.push_error_handler(Some(EvalErrorHandlerState {
            callback: cells[4], levels: -1,
        }));
        context.push_error_handler(Some(EvalErrorHandlerState {
            callback: cells[3], levels: -1,
        }));
        context.push_exception_handler(Some(cells[6]));
        context.push_exception_handler(Some(cells[5]));

        assert!(context.has_context_cell_owners());

        let drained = context.take_context_cell_owners();
        for cell in cells {
            assert_eq!(drained.iter().filter(|owner| *owner == cell).count(), 1);
        }
        assert!(context.take_context_cell_owners().is_empty());
        assert!(!context.has_context_cell_owners());
    }

    /// A newly allocated cell cannot inherit the cursor of a retired cell at the same address.
    #[test]
    fn array_cursor_is_bound_to_the_exact_cell_lifetime() {
        let mut context = ElephcEvalContext::new();
        let address = usize::MAX - 211;
        let cell = RuntimeCellHandle::from_raw(address as *mut crate::value::RuntimeCell);
        context.set_array_cursor(cell, EvalArrayCursor::Position(4));
        assert_eq!(context.array_cursor(cell), EvalArrayCursor::Position(4));

        elephc_memoir::cell_retirement::retire(address);
        assert_eq!(context.array_cursor(cell), EvalArrayCursor::Position(0));
        context.set_array_cursor(cell, EvalArrayCursor::Position(2));
        assert_eq!(context.array_cursor(cell), EvalArrayCursor::Position(2));
        elephc_memoir::cell_retirement::retire(address);
    }

    /// Destructor callbacks cannot recursively claim a context already undergoing teardown.
    #[test]
    fn persistent_context_teardown_survives_nested_abi_owners() {
        let mut context = ElephcEvalContext::new();
        assert!(!context.begin_teardown());
        assert!(context.release_abi_owner());
        assert!(context.begin_teardown());
        assert!(!context.begin_teardown());
        context.retain_abi_owner().unwrap();
        assert!(context.release_abi_owner());
        assert!(context.teardown_active());
        context.end_teardown();
        assert!(context.begin_teardown());
    }
}
