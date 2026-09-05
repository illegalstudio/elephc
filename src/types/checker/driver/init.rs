//! Purpose:
//! Implements the checker driver init phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use crate::types::predefined_constants::{php_type_of, registered_constants};
use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;

use super::super::Checker;

impl Checker {
    /// Constructs a new `Checker` with pre-populated builtin constants and empty declaration tables.
    ///
    /// Initializes the global constant map with PHP built-in constants (`PHP_OS`, the
    /// `PHP_VERSION*` / `PHP_SAPI` version surface, `SID`, pathinfo
    /// constants, `ENT_*` HTML-escaping flags, `FNM_*` flags, stream resources, and lock flags),
    /// array, JSON, stream, date, and preg constants, `PHP_SESSION_*`
    /// session-status constants, and `E_*` error-level constants. All other tables (function declarations,
    /// classes, interfaces, enums, etc.) are initialized empty.
    ///
    /// # Arguments
    /// * `target_platform` - The compilation target platform, stored for use in platform-specific
    ///   type checks and library requirements.
    ///
    /// # Returns
    /// A `Checker` instance ready for the program to be loaded into.
    pub(super) fn new(target: Target) -> Self {
        // Every unconditionally registered constant comes from the shared catalog; only its
        // TYPE is declared here, the value is baked per compilation by
        // `codegen_support::prescan::collect_constants`.
        let mut constants = HashMap::new();
        for constant in registered_constants() {
            constants.insert(constant.name.to_string(), php_type_of(constant.value));
        }

        Self {
            target,
            fn_decls: HashMap::new(),
            function_variant_groups: HashMap::new(),
            functions: HashMap::new(),
            resolving_functions: HashSet::new(),
            constants,
            closure_return_types: HashMap::new(),
            callable_sigs: HashMap::new(),
            callable_param_names: HashSet::new(),
            callable_param_sigs: HashMap::new(),
            strict_types: false,
            param_specialization_seen: HashSet::new(),
            callable_return_sigs: HashMap::new(),
            callable_array_return_sigs: HashMap::new(),
            callable_captures: HashMap::new(),
            callable_array_targets: HashMap::new(),
            first_class_callable_targets: HashMap::new(),
            reflection_class_targets: HashMap::new(),
            interfaces: HashMap::new(),
            classes: HashMap::new(),
            declared_classes: HashSet::new(),
            enums: HashMap::new(),
            declared_interfaces: HashSet::new(),
            declared_traits: HashSet::new(),
            declared_trait_methods: HashMap::new(),
            declared_trait_constants: HashMap::new(),
            current_class: None,
            current_method: None,
            current_function: None,
            current_method_is_static: false,
            current_by_ref_return: false,
            closure_depth: 0,
            extern_functions: HashMap::new(),
            extern_classes: HashMap::new(),
            packed_classes: HashMap::new(),
            extern_globals: HashMap::new(),
            required_libraries: Vec::new(),
            top_level_env: HashMap::new(),
            active_ref_params: HashSet::new(),
            active_globals: HashSet::new(),
            // Filled by `check_types_impl` from the whole program before the first walk; an empty
            // set here just means "no `global` declaration is known", which is the safe default
            // for the handful of tests that build a `Checker` directly.
            program_global_names: HashSet::new(),
            active_statics: HashSet::new(),
            foreach_key_locals: HashSet::new(),
            eval_barrier_active: false,
            flow_typed_returns: HashMap::new(),
            null_probe_scope_is_top_level: false,
            pending_null_probe_roots: Vec::new(),
            null_probe_depth: 0,
            break_continue_depth: 0,
            finally_break_continue_bases: Vec::new(),
            current_loop_storage_scope: "main".to_string(),
            warnings: Vec::new(),
            reference_property_promotions: HashSet::new(),
            throw_access_sites: HashMap::new(),
            builtin_call_types: HashMap::new(),
            loop_storage_types: HashMap::new(),
            string_incdec_locals: HashSet::new(),
            strict_locals: false,
            local_conditional_depth: 0,
            local_binding_depth: HashMap::new(),
            ref_aliased_locals: HashSet::new(),
            static_local_names: HashSet::new(),
            typed_local_names: HashSet::new(),
            local_bind_kill_sites: HashMap::new(),
            local_retype_sites: HashMap::new(),
            statement_position_expr: None,
            body_contains_eval: false,
            mixed_storage_locals: HashSet::new(),
            mixed_storage_store_sites: HashMap::new(),
            binding_decision_warnings: HashMap::new(),
            retired_mixed_storage_store_sites: HashSet::new(),
        }
    }
}
