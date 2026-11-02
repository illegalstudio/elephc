//! Purpose:
//! Defines the opaque eval context storage layout and initializes all registries.
//!
//! Called from:
//! - Public context construction and every context method family.
//!
//! Key details:
//! - Generated code only passes this value opaquely; Rust owns every internal collection.

use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use crate::errors::EvalStatus;

/// One installed eval user error handler and its accepted level mask.
#[derive(Clone, Copy)]
pub(crate) struct EvalErrorHandlerState {
    pub(crate) callback: RuntimeCellHandle,
    pub(crate) levels: i64,
}

/// One PHP handler retained by Magician for a process signal.
#[derive(Clone, Copy)]
pub enum EvalPcntlSignalHandler {
    /// Native `SIG_DFL` or `SIG_IGN` disposition value.
    Disposition(i64),
    /// Runtime callable cell retained while the handler remains registered.
    Callable(RuntimeCellHandle),
}

/// How much of the process-global class registry one context has already imported.
///
/// The counts are prefix lengths into the registry's push-only name lists; `generation` is the
/// registry's own, and a mismatch means the lists were emptied and every count is meaningless.
#[derive(Clone, Copy, Default)]
pub(super) struct GlobalEvalSyncMark {
    pub(super) generation: u64,
    pub(super) classes: usize,
    pub(super) interfaces: usize,
    pub(super) traits: usize,
    pub(super) enums: usize,
    pub(super) aliases: usize,
}

/// Process-level eval context passed opaquely across the C ABI.
///
/// Generated code never inspects this layout directly; it only passes pointers
/// back to the eval bridge. Keeping a concrete Rust type here lets the bridge
/// grow dynamic registries without exposing them to generated assembly.
pub struct ElephcEvalContext {
    pub(super) abi_version: u32,
    pub(crate) native_global_sync: Option<NativeGlobalSyncHooks>,
    pub(super) abi_owner_count: usize,
    pub(super) native_catalog_generation: u64,
    pub(super) native_catalog_ready: bool,
    pub(super) native_catalog_registration_failed: bool,
    /// Numeric evals admitted without catalog registration in this activation.
    pub(super) native_catalog_deferrals: u16,
    /// Sticky fallback after one fragment can retain or invoke indirect scope state.
    pub(super) scope_sync_requires_full: bool,
    /// SHARED, not owned: a context imports class-likes another context declared on every
    /// `sync_global_eval_classes`, and a deep copy of each `EvalClass` -- method bodies and
    /// all -- was ~13% of a Symfony request, paid again on teardown when the request-scoped
    /// context dropped every copy.
    pub(super) classes: rustc_hash::FxHashMap<String, Arc<EvalClass>>,
    /// How much of the process-global class registry this context has already imported.
    pub(super) global_eval_sync: GlobalEvalSyncMark,
    pub(super) class_source_files: HashMap<String, String>,
    pub(super) class_aliases: rustc_hash::FxHashMap<String, EvalClassAlias>,
    /// Names declared before this context adopted the snapshot: SHARED, never written.
    pub(super) declared_class_names: SharedNative<Vec<String>>,
    /// Names this context declared itself, appended without copying the shared prefix.
    pub(super) own_declared_class_names: Vec<String>,
    pub(super) interfaces: rustc_hash::FxHashMap<String, Arc<EvalInterface>>,
    /// Names declared before this context adopted the snapshot: SHARED, never written.
    pub(super) declared_interface_names: SharedNative<Vec<String>>,
    /// Names this context declared itself, appended without copying the shared prefix.
    pub(super) own_declared_interface_names: Vec<String>,
    pub(super) traits: rustc_hash::FxHashMap<String, Arc<EvalTrait>>,
    /// Names declared before this context adopted the snapshot: SHARED, never written.
    pub(super) declared_trait_names: SharedNative<Vec<String>>,
    /// Names this context declared itself, appended without copying the shared prefix.
    pub(super) own_declared_trait_names: Vec<String>,
    pub(super) enums: rustc_hash::FxHashMap<String, Arc<EvalEnum>>,
    pub(super) declared_enum_names: Vec<String>,
    pub(super) enum_cases: HashMap<(String, String), RuntimeCellHandle>,
    pub(super) enum_case_values: HashMap<(String, String), RuntimeCellHandle>,
    pub(super) constants: HashMap<String, RuntimeCellHandle>,
    pub(super) native_global_constants: SharedNative<HashMap<String, EvalNativeGlobalConstant>>,
    pub(super) native_user_constants: SharedNative<HashMap<String, EvalNativeUserConstant>>,
    pub(super) generic_templates: HashMap<String, crate::eval_ir::EvalGenericTemplate>,
    pub(super) generic_specializations: HashSet<String>,
    pub(super) generic_methods: HashMap<String, crate::eval_ir::EvalClassMethod>,
    pub(super) local_reference_types: HashMap<usize, EvalLocalReferenceTypes>,
    pub(super) local_reference_retirements: crate::ffi::array_references::ArrayReferenceRetirements,
    pub(super) functions: HashMap<String, EvalFunction>,
    pub(super) closures: HashMap<String, EvalClosure>,
    pub(super) closure_objects: HashMap<u64, EvalClosureObjectTarget>,
    /// Initializers of lazy ghosts that no property access has triggered yet, by object identity.
    pub(super) lazy_ghost_initializers: HashMap<u64, RuntimeCellHandle>,
    pub(super) live_closure_objects: AtomicUsize,
    pub(super) live_dynamic_objects: AtomicUsize,
    pub(super) live_global_functions: AtomicUsize,
    pub(super) live_autoload_contexts: AtomicUsize,
    pub(super) retained_context_free_requested: AtomicBool,
    pub(super) next_closure_id: usize,
    pub(super) native_functions: SharedNative<HashMap<String, NativeFunction>>,
    /// Native functions THIS context registered after adopting the shared AOT table.
    ///
    /// WHY. `native_functions` is the module's whole function table, shared between every
    /// context through the AOT snapshot. Reflecting a compiled closure registers it under a
    /// per-descriptor name, and inserting that into the shared table meant `Arc::make_mut`
    /// copied the entire table -- thousands of entries -- for the first such registration in
    /// every context; on a Laravel request that copy alone was a sixth of the work. Late
    /// registrations go here instead, and lookups consult this overlay before the shared table.
    pub(super) local_native_functions: HashMap<String, NativeFunction>,
    pub(super) native_methods: SharedNative<HashMap<(String, String), NativeCallableSignature>>,
    pub(super) native_static_methods: SharedNative<HashMap<(String, String), NativeCallableSignature>>,
    pub(super) native_constructors: SharedNative<HashMap<String, NativeCallableSignature>>,
    pub(super) native_class_parents: SharedNative<HashMap<String, String>>,
    pub(super) native_class_attributes: SharedNative<HashMap<String, Vec<EvalAttribute>>>,
    pub(super) native_method_attributes: SharedNative<HashMap<(String, String), Vec<EvalAttribute>>>,
    pub(super) native_constant_attributes: SharedNative<HashMap<(String, String), Vec<EvalAttribute>>>,
    pub(super) native_interface_properties: SharedNative<HashMap<String, Vec<(String, EvalInterfaceProperty)>>>,
    pub(super) native_abstract_properties: SharedNative<HashMap<String, Vec<(String, EvalInterfaceProperty)>>>,
    pub(super) native_property_types: SharedNative<HashMap<(String, String), EvalParameterType>>,
    pub(super) native_property_defaults: SharedNative<HashMap<(String, String), NativeCallableDefault>>,
    pub(super) native_property_default_errors: SharedNative<HashMap<(String, String), String>>,
    pub(super) native_class_default_errors: SharedNative<HashMap<String, String>>,
    pub(super) native_property_attributes: SharedNative<HashMap<(String, String), Vec<EvalAttribute>>>,
    /// Array-valued compiled class constants, which the generated scalar constant helper cannot
    /// box. Keyed like `native_constant_key`; see `define_native_class_constant_default`.
    pub(super) native_class_constant_defaults:
        SharedNative<HashMap<(String, String), NativeClassConstantDefault>>,
    /// Overrides the key `static` slots hang from, for callables the function name cannot name.
    ///
    /// Only closures need it: `__FUNCTION__` and a backtrace must keep saying `{closure}`, which
    /// is the FUNCTION name, while the statics belong to this closure OBJECT.
    pub(super) static_slot_keys: Vec<String>,
    pub(super) static_locals: HashMap<(String, String), RuntimeCellHandle>,
    pub(super) static_properties: HashMap<(String, String), RuntimeCellHandle>,
    pub(super) static_property_aliases: HashMap<(String, String), EvalReferenceTarget>,
    pub(super) class_constants: HashMap<(String, String), RuntimeCellHandle>,
    pub(super) include_state: super::request_includes::SharedIncludeState,
    pub(super) claimed_aot_include_classlikes: HashSet<String>,
    pub(super) include_execution_stack: Vec<bool>,
    pub(super) included_file_order: Vec<String>,
    pub(super) included_main_file: Option<String>,
    pub(super) error_reporting: i64,
    pub(super) error_handler: Option<EvalErrorHandlerState>,
    pub(super) error_handler_stack: Vec<Option<EvalErrorHandlerState>>,
    pub(super) exception_handler: Option<RuntimeCellHandle>,
    pub(super) exception_handler_stack: Vec<Option<RuntimeCellHandle>>,
    pub(super) dynamic_objects: HashMap<u64, String>,
    pub(super) dynamic_destructing_objects: HashSet<u64>,
    pub(super) dynamic_destructed_objects: HashSet<u64>,
    pub(super) dynamic_property_values: HashMap<(u64, String), RuntimeCellHandle>,
    /// Overlay property names per object in the order they were first written.
    ///
    /// PHP reports an object's dynamic properties in creation order, and the overlay map
    /// above cannot answer that. Everything that ENUMERATES an object reads its order from
    /// here, so the answer is PHP's rather than a hash order or an alphabetical stand-in.
    pub(super) dynamic_property_order: HashMap<u64, Vec<String>>,
    pub(super) clone_reinitialized_properties: HashMap<u64, HashSet<String>>,
    pub(super) dynamic_property_aliases: HashMap<(u64, String), EvalReferenceTarget>,
    pub(super) array_element_aliases: HashMap<usize, EvalArrayReferenceAliases>,
    pub(super) array_reference_retirements: crate::ffi::array_references::ArrayReferenceRetirements,
    pub(super) array_cursors: HashMap<usize, EvalArrayCursor>,
    pub(super) pcntl_foreign_callables: HashMap<
        usize,
        (RuntimeCellHandle, pcntl_runtime::EvalPcntlContextLease),
    >,
    /// Sticky capability: foreign callable metadata has entered this context.
    pub(super) may_have_foreign_pcntl_callables: bool,
    pub(super) dynamic_initialized_properties: HashSet<(u64, String)>,
    /// Live interpreter call frames, innermost last, for `debug_backtrace()`.
    /// Execution state of every live generator, keyed by its object identity.
    ///
    /// A generator outlives the call that created it, so its frame — including its own
    /// scope — cannot live on that call's stack.
    pub(super) eval_generators: HashMap<u64, EvalGeneratorFrame>,
    pub(super) eval_reflection_attributes: HashMap<u64, EvalReflectionAttributeMetadata>,
    pub(super) eval_reflection_classes: HashMap<u64, String>,
    pub(super) eval_reflection_functions: HashMap<u64, String>,
    pub(super) eval_reflection_function_closure_targets: HashMap<u64, EvalClosureObjectTarget>,
    pub(super) eval_reflection_methods: HashMap<u64, (String, String)>,
    pub(super) eval_reflection_properties: HashMap<u64, (String, String)>,
    pub(super) eval_dynamic_reflection_properties: HashSet<u64>,
    pub(super) eval_reflection_class_constants: HashMap<u64, (String, String, u64)>,
    pub(super) eval_static_callables: HashMap<usize, EvalStaticCallableMetadata>,
    pub(super) eval_object_callables: HashMap<usize, EvalObjectCallableMetadata>,
    pub(super) global_scope: Option<*mut ElephcEvalScope>,
    pub(super) owns_global_scope: bool,
    pub(super) autoload_callbacks: Vec<(i64, RuntimeCellHandle)>,
    pub(super) autoloading_classes: HashSet<String>,
    pub(super) function_stack: Vec<String>,
    pub(super) returns_by_ref: bool,
    pub(super) strict_types: bool,
    /// `declare(ticks=N)`'s N, or 0 when no tick directive is in force.
    pub(super) tick_interval: i64,
    /// Statements executed since the last tick fired.
    pub(super) tick_counter: i64,
    /// Callables `register_tick_function()` has registered, in registration order.
    pub(super) tick_functions: Vec<RuntimeCellHandle>,
    /// Whether a tick handler is running, so its own statements do not tick again.
    pub(super) tick_running: bool,
    pub(super) array_iterators: HashMap<u64, EvalArrayIteratorState>,
    pub(super) pending_return_reference: Option<(EvalReferenceTarget, RuntimeCellHandle)>,
    pub(super) function_args_stack: Vec<EvalBacktraceFrame>,
    pub(super) eval_backtrace_boundaries: Vec<(usize, EvalBacktraceFrame)>,
    pub(super) class_stack: Vec<String>,
    pub(super) called_class_stack: Vec<String>,
    pub(super) magic_stack: Vec<EvalMagicScope>,
    pub(super) pending_throw: Option<RuntimeCellHandle>,
    pub(super) spl_autoload_extensions: String,
    pub(super) streams: EvalStreamResources,
    pub(super) json_last_error: i64,
    pub(super) json_last_error_msg: String,
    pub(super) default_timezone: String,
    pub(super) http_response_code: i64,
    pub(super) call_file: String,
    pub(super) call_dir: String,
    pub(super) call_line: i64,
    pub(super) file_magic_override: Option<String>,
    pub(super) error_suppression_depth: usize,
    /// Depth of nested class-like-member-default evaluation (constant initializers, property
    /// defaults, enum case values). `static::` has no meaning there -- php refuses it as an
    /// uncatchable compile-time error -- so `resolve_eval_static_class_name` consults this
    /// instead of silently falling back to `self` the way a live method call frame would.
    pub(super) compile_time_constant_depth: usize,
    /// Class-like constants currently being evaluated on this call chain, keyed by
    /// `(declaring_class, constant_name)`. Constant initializers are lazily materialized and
    /// cached on first read (`eval_class_like_constant_cell`); a constant whose initializer
    /// reads itself, directly or through another constant, would otherwise recurse without
    /// bound instead of raising php's catchable `Cannot declare self-referencing constant`.
    pub(super) evaluating_class_constants: HashSet<(String, String)>,
}

impl ElephcEvalContext {
    /// Creates a context using the current eval bridge ABI version.
    pub fn new() -> Self {
        Self {
            abi_version: ABI_VERSION,
            native_global_sync: None,
            abi_owner_count: 1,
            native_catalog_generation: 0,
            native_catalog_ready: false,
            native_catalog_registration_failed: false,
            native_catalog_deferrals: 0,
            scope_sync_requires_full: false,
            classes: rustc_hash::FxHashMap::default(),
            global_eval_sync: GlobalEvalSyncMark::default(),
            class_source_files: HashMap::new(),
            class_aliases: rustc_hash::FxHashMap::default(),
            declared_class_names: SharedNative::default(),
            own_declared_class_names: Vec::new(),
            interfaces: rustc_hash::FxHashMap::default(),
            declared_interface_names: SharedNative::default(),
            own_declared_interface_names: Vec::new(),
            traits: rustc_hash::FxHashMap::default(),
            declared_trait_names: SharedNative::default(),
            own_declared_trait_names: Vec::new(),
            enums: rustc_hash::FxHashMap::default(),
            declared_enum_names: Vec::new(),
            enum_cases: HashMap::new(),
            enum_case_values: HashMap::new(),
            constants: HashMap::new(),
            native_global_constants: SharedNative::default(),
            native_user_constants: SharedNative::default(),
            generic_templates: HashMap::new(),
            generic_specializations: HashSet::new(),
            generic_methods: HashMap::new(),
            local_reference_types: HashMap::new(),
            local_reference_retirements: Default::default(),
            functions: HashMap::new(),
            closures: HashMap::new(),
            closure_objects: HashMap::new(),
            lazy_ghost_initializers: HashMap::new(),
            live_closure_objects: AtomicUsize::new(0),
            live_dynamic_objects: AtomicUsize::new(0),
            live_global_functions: AtomicUsize::new(0),
            live_autoload_contexts: AtomicUsize::new(0),
            retained_context_free_requested: AtomicBool::new(false),
            next_closure_id: 0,
            native_functions: SharedNative::default(),
            local_native_functions: HashMap::new(),
            native_methods: SharedNative::default(),
            native_static_methods: SharedNative::default(),
            native_constructors: SharedNative::default(),
            native_class_parents: SharedNative::default(),
            native_class_attributes: SharedNative::default(),
            native_method_attributes: SharedNative::default(),
            native_constant_attributes: SharedNative::default(),
            native_interface_properties: SharedNative::default(),
            native_abstract_properties: SharedNative::default(),
            native_property_types: SharedNative::default(),
            native_property_defaults: SharedNative::default(),
            native_property_default_errors: SharedNative::default(),
            native_class_default_errors: SharedNative::default(),
            native_property_attributes: SharedNative::default(),
            native_class_constant_defaults: SharedNative::default(),
            static_slot_keys: Vec::new(),
            static_locals: HashMap::new(),
            static_properties: HashMap::new(),
            static_property_aliases: HashMap::new(),
            class_constants: HashMap::new(),
            include_state: super::request_includes::current_include_state(),
            claimed_aot_include_classlikes: HashSet::new(),
            include_execution_stack: Vec::new(),
            included_file_order: Vec::new(),
            included_main_file: None,
            error_reporting: crate::eval_php_profile::eval_all_error_mask(),
            error_handler: None,
            error_handler_stack: Vec::new(),
            exception_handler: None,
            exception_handler_stack: Vec::new(),
            dynamic_objects: HashMap::new(),
            dynamic_destructing_objects: HashSet::new(),
            dynamic_destructed_objects: HashSet::new(),
            dynamic_property_values: HashMap::new(),
            dynamic_property_order: HashMap::new(),
            clone_reinitialized_properties: HashMap::new(),
            dynamic_property_aliases: HashMap::new(),
            array_element_aliases: HashMap::new(),
            array_reference_retirements: Default::default(),
            array_cursors: HashMap::new(),
            pcntl_foreign_callables: HashMap::new(),
            may_have_foreign_pcntl_callables: false,
            dynamic_initialized_properties: HashSet::new(),
            eval_generators: HashMap::new(),
            eval_reflection_attributes: HashMap::new(),
            eval_reflection_classes: HashMap::new(),
            eval_reflection_functions: HashMap::new(),
            eval_reflection_function_closure_targets: HashMap::new(),
            eval_reflection_methods: HashMap::new(),
            eval_reflection_properties: HashMap::new(),
            eval_dynamic_reflection_properties: HashSet::new(),
            eval_reflection_class_constants: HashMap::new(),
            eval_static_callables: HashMap::new(),
            eval_object_callables: HashMap::new(),
            global_scope: None,
            owns_global_scope: false,
            autoload_callbacks: Vec::new(),
            autoloading_classes: HashSet::new(),
            function_stack: Vec::new(),
            returns_by_ref: false,
            strict_types: false,
            tick_interval: 0,
            tick_counter: 0,
            tick_functions: Vec::new(),
            tick_running: false,
            array_iterators: HashMap::new(),
            pending_return_reference: None,
            function_args_stack: Vec::new(),
            eval_backtrace_boundaries: Vec::new(),
            class_stack: Vec::new(),
            called_class_stack: Vec::new(),
            magic_stack: Vec::new(),
            pending_throw: None,
            spl_autoload_extensions: String::from(".inc,.php"),
            streams: EvalStreamResources::default(),
            json_last_error: 0,
            json_last_error_msg: String::from("No error"),
            default_timezone: String::from("UTC"),
            http_response_code: 200,
            call_file: String::new(),
            call_dir: String::new(),
            call_line: 0,
            file_magic_override: None,
            error_suppression_depth: 0,
            compile_time_constant_depth: 0,
            evaluating_class_constants: HashSet::new(),
        }
    }

    /// Creates a context with an explicit ABI version for compatibility tests.
    #[cfg(test)]
    pub fn for_abi_version(abi_version: u32) -> Self {
        Self {
            abi_version,
            native_global_sync: None,
            abi_owner_count: 1,
            native_catalog_generation: 0,
            native_catalog_ready: false,
            native_catalog_registration_failed: false,
            native_catalog_deferrals: 0,
            scope_sync_requires_full: false,
            classes: rustc_hash::FxHashMap::default(),
            global_eval_sync: GlobalEvalSyncMark::default(),
            class_source_files: HashMap::new(),
            class_aliases: rustc_hash::FxHashMap::default(),
            declared_class_names: SharedNative::default(),
            own_declared_class_names: Vec::new(),
            interfaces: rustc_hash::FxHashMap::default(),
            declared_interface_names: SharedNative::default(),
            own_declared_interface_names: Vec::new(),
            traits: rustc_hash::FxHashMap::default(),
            declared_trait_names: SharedNative::default(),
            own_declared_trait_names: Vec::new(),
            enums: rustc_hash::FxHashMap::default(),
            declared_enum_names: Vec::new(),
            enum_cases: HashMap::new(),
            enum_case_values: HashMap::new(),
            constants: HashMap::new(),
            native_global_constants: SharedNative::default(),
            native_user_constants: SharedNative::default(),
            generic_templates: HashMap::new(),
            generic_specializations: HashSet::new(),
            generic_methods: HashMap::new(),
            local_reference_types: HashMap::new(),
            local_reference_retirements: Default::default(),
            functions: HashMap::new(),
            closures: HashMap::new(),
            closure_objects: HashMap::new(),
            lazy_ghost_initializers: HashMap::new(),
            live_closure_objects: AtomicUsize::new(0),
            live_dynamic_objects: AtomicUsize::new(0),
            live_global_functions: AtomicUsize::new(0),
            live_autoload_contexts: AtomicUsize::new(0),
            retained_context_free_requested: AtomicBool::new(false),
            next_closure_id: 0,
            native_functions: SharedNative::default(),
            local_native_functions: HashMap::new(),
            native_methods: SharedNative::default(),
            native_static_methods: SharedNative::default(),
            native_constructors: SharedNative::default(),
            native_class_parents: SharedNative::default(),
            native_class_attributes: SharedNative::default(),
            native_method_attributes: SharedNative::default(),
            native_constant_attributes: SharedNative::default(),
            native_interface_properties: SharedNative::default(),
            native_abstract_properties: SharedNative::default(),
            native_property_types: SharedNative::default(),
            native_property_defaults: SharedNative::default(),
            native_property_default_errors: SharedNative::default(),
            native_class_default_errors: SharedNative::default(),
            native_property_attributes: SharedNative::default(),
            native_class_constant_defaults: SharedNative::default(),
            static_slot_keys: Vec::new(),
            static_locals: HashMap::new(),
            static_properties: HashMap::new(),
            static_property_aliases: HashMap::new(),
            class_constants: HashMap::new(),
            include_state: super::request_includes::current_include_state(),
            claimed_aot_include_classlikes: HashSet::new(),
            include_execution_stack: Vec::new(),
            included_file_order: Vec::new(),
            included_main_file: None,
            error_reporting: crate::eval_php_profile::eval_all_error_mask(),
            error_handler: None,
            error_handler_stack: Vec::new(),
            exception_handler: None,
            exception_handler_stack: Vec::new(),
            dynamic_objects: HashMap::new(),
            dynamic_destructing_objects: HashSet::new(),
            dynamic_destructed_objects: HashSet::new(),
            dynamic_property_values: HashMap::new(),
            dynamic_property_order: HashMap::new(),
            clone_reinitialized_properties: HashMap::new(),
            dynamic_property_aliases: HashMap::new(),
            array_element_aliases: HashMap::new(),
            array_reference_retirements: Default::default(),
            array_cursors: HashMap::new(),
            pcntl_foreign_callables: HashMap::new(),
            may_have_foreign_pcntl_callables: false,
            dynamic_initialized_properties: HashSet::new(),
            eval_generators: HashMap::new(),
            eval_reflection_attributes: HashMap::new(),
            eval_reflection_classes: HashMap::new(),
            eval_reflection_functions: HashMap::new(),
            eval_reflection_function_closure_targets: HashMap::new(),
            eval_reflection_methods: HashMap::new(),
            eval_reflection_properties: HashMap::new(),
            eval_dynamic_reflection_properties: HashSet::new(),
            eval_reflection_class_constants: HashMap::new(),
            eval_static_callables: HashMap::new(),
            eval_object_callables: HashMap::new(),
            global_scope: None,
            owns_global_scope: false,
            autoload_callbacks: Vec::new(),
            autoloading_classes: HashSet::new(),
            function_stack: Vec::new(),
            returns_by_ref: false,
            strict_types: false,
            tick_interval: 0,
            tick_counter: 0,
            tick_functions: Vec::new(),
            tick_running: false,
            array_iterators: HashMap::new(),
            pending_return_reference: None,
            function_args_stack: Vec::new(),
            eval_backtrace_boundaries: Vec::new(),
            class_stack: Vec::new(),
            called_class_stack: Vec::new(),
            magic_stack: Vec::new(),
            pending_throw: None,
            spl_autoload_extensions: String::from(".inc,.php"),
            streams: EvalStreamResources::default(),
            json_last_error: 0,
            json_last_error_msg: String::from("No error"),
            default_timezone: String::from("UTC"),
            http_response_code: 200,
            call_file: String::new(),
            call_dir: String::new(),
            call_line: 0,
            file_magic_override: None,
            error_suppression_depth: 0,
            compile_time_constant_depth: 0,
            evaluating_class_constants: HashSet::new(),
        }
    }

    /// Returns the ABI version this context was created for.
    pub const fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Enters one nested PHP error-suppression (`@`) expression scope.
    pub fn push_error_suppression(&mut self) {
        self.error_suppression_depth += 1;
    }

    /// Leaves one nested PHP error-suppression (`@`) expression scope.
    pub fn pop_error_suppression(&mut self) {
        self.error_suppression_depth = self.error_suppression_depth.saturating_sub(1);
    }

    /// Reports whether the current eval expression suppresses non-fatal diagnostics.
    pub const fn errors_suppressed(&self) -> bool {
        self.error_suppression_depth != 0
    }

    /// Enters one `isset`/`empty`/`??`/`??=` operand scope, where an uninitialized typed
    /// property ANSWERS instead of raising.
    ///
    /// This is not error suppression. `@` silences a diagnostic that is still produced, and it
    /// propagates into calls; PHP's quiet fetch is a different FETCH MODE that answers "absent"
    /// without ever performing the read, and it stops at a call boundary. Measured against
    /// `php -n` 8.5.6: `$n->leaf->t ?? 'D'` answers `'D'` with `Node::$leaf` itself
    /// uninitialized, so the mode reaches down a whole property/dim chain, while
    /// `readsUninit($n->leaf) ?? 'D'` and `$n->leaf->getT() ?? 'D'` both THROW, so it must not
    /// reach into a call. `enter_call_barrier` is what implements that second half.
    pub fn push_quiet_property_fetch(&mut self) {
        push_thread_quiet_property_fetch();
    }

    /// Leaves one quiet-fetch operand scope.
    pub fn pop_quiet_property_fetch(&mut self) {
        pop_thread_quiet_property_fetch();
    }

    /// Reports whether an uninitialized typed property should answer rather than raise.
    pub fn quiet_property_fetch(&self) -> bool {
        thread_quiet_property_fetch()
    }

    /// Suspends the quiet-fetch mode for the duration of a call, returning the depth to restore.
    ///
    /// PHP's quiet fetch does not cross into a function or method body: a callee that reads an
    /// uninitialized typed property raises even when the CALL sits in the operand of `??`.
    #[must_use]
    pub fn enter_call_barrier(&mut self) -> usize {
        take_thread_quiet_property_fetch()
    }

    /// Restores the quiet-fetch depth saved by `enter_call_barrier`.
    pub fn leave_call_barrier(&mut self, saved: usize) {
        set_thread_quiet_property_fetch(saved);
    }

    /// Adds one opaque ABI owner that can keep this heap context alive past its frame.
    pub(crate) fn retain_abi_owner(&mut self) -> Result<(), EvalStatus> {
        self.abi_owner_count = self
            .abi_owner_count
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        Ok(())
    }

    /// Reports whether a deferred context acquired a new opaque ABI callback owner.
    pub(crate) fn has_abi_owners(&self) -> bool {
        self.abi_owner_count != 0
    }

    /// Drops one opaque ABI owner and reports whether the heap context can be destroyed.
    pub(crate) fn release_abi_owner(&mut self) -> bool {
        if self.abi_owner_count == 0 {
            return false;
        }
        self.abi_owner_count -= 1;
        self.abi_owner_count == 0
    }

}

/// Enters one quiet-fetch operand scope on this thread.
///
/// The free-function form is what COMPILED code reaches through
/// `__elephc_eval_quiet_property_fetch_push`: it has no eval context in hand at that point, and
/// does not need one, because the depth belongs to the thread rather than to any context.
pub fn push_thread_quiet_property_fetch() {
    QUIET_PROPERTY_FETCH_DEPTH.with(|depth| depth.set(depth.get() + 1));
}

/// Leaves one quiet-fetch operand scope on this thread.
pub fn pop_thread_quiet_property_fetch() {
    QUIET_PROPERTY_FETCH_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
}

/// Reports whether this thread is inside a quiet-fetch operand scope.
pub fn thread_quiet_property_fetch() -> bool {
    QUIET_PROPERTY_FETCH_DEPTH.with(|depth| depth.get() != 0)
}

/// Suspends the quiet-fetch mode across a call, returning the depth to restore.
pub fn take_thread_quiet_property_fetch() -> usize {
    QUIET_PROPERTY_FETCH_DEPTH.with(|depth| depth.replace(0))
}

/// Restores a quiet-fetch depth taken by `take_thread_quiet_property_fetch`.
pub fn set_thread_quiet_property_fetch(saved: usize) {
    QUIET_PROPERTY_FETCH_DEPTH.with(|depth| depth.set(saved));
}

thread_local! {
    /// Depth of the enclosing quiet-fetch operand scopes for THIS request.
    ///
    /// Deliberately not a field on the context. A single PHP expression routinely crosses
    /// several eval contexts -- `isset($this->p[$k])` inside an included file reaches an
    /// AOT-declared class through the bridge -- and a per-context counter would go quiet on the
    /// context that entered `isset()` while the context that performs the read still raised.
    /// A request runs on one thread and a forked web worker gets its own copy, so the execution
    /// stack the mode belongs to is exactly thread-local, the same reasoning the call-frame
    /// stack already follows.
    static QUIET_PROPERTY_FETCH_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
