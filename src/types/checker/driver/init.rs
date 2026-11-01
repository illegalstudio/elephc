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
use crate::types::pcntl_constants::pcntl_int_constants;
use crate::types::token_constants::TOKEN_INT_CONSTANTS;
use crate::types::PhpType;

use super::super::Checker;

impl Checker {
    /// Constructs a new `Checker` with pre-populated builtin constants and empty declaration tables.
    ///
    /// Initializes the global constant map from the shared catalog, which owns every
    /// unconditionally predefined name, plus the two families it cannot: the target-aware PCNTL
    /// signal/errno table and the profile-aware `T_*` tokenizer table. All other tables
    /// (function declarations, classes, interfaces, enums, etc.) are initialized empty.
    ///
    /// # Arguments
    /// * `target` - The full compilation target, stored for platform- and Apple-variant-specific
    ///   type checks and library requirements.
    ///
    /// # Returns
    /// A `Checker` instance ready for the program to be loaded into.
    pub(crate) fn new(target: Target) -> Self {
        // Every unconditionally registered constant comes from the shared catalog; only its
        // TYPE is declared here, the value is baked per compilation by
        // `codegen_support::prescan::collect_constants`.
        let mut constants = HashMap::new();
        for constant in registered_constants() {
            constants.insert(constant.name.to_string(), php_type_of(constant.value));
        }
        for (name, _) in pcntl_int_constants(target) {
            constants.insert(name.to_string(), PhpType::Int);
        }
        // `T_*` is profile-dependent, so it cannot live in the shared catalog; `prescan`
        // registers the same table for codegen.
        for (name, _values) in TOKEN_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }

        Self {
            target,
            fn_decls: HashMap::new(),
            folded_function_names: Default::default(),
            canonical_function_names: Default::default(),
            function_variant_groups: HashMap::new(),
            failed_variant_contracts: HashMap::new(),
            completed_function_signatures: HashSet::new(),
            functions: HashMap::new(),
            resolving_functions: HashSet::new(),
            recursive_param_widenings: HashMap::new(),
            functions_called_directly: HashSet::new(),
            constants,
            closure_return_types: HashMap::new(),
            callable_sigs: HashMap::new(),
            static_property_callable_sigs: HashMap::new(),
            callable_param_names: HashSet::new(),
            callable_param_sigs: HashMap::new(),
            callable_param_site_sigs: HashMap::new(),
            strict_types: false,
            internal_callback_binding: false,
            param_specialization_seen: HashSet::new(),
            class_children_index: None,
            method_impl_users_index: None,
            property_sharers_index: None,
            interface_method_boxed_params: HashSet::new(),
            requested_instantiations: Vec::new(),
            generic_call_sites: HashMap::new(),
            class_templates: Vec::new(),
            method_templates: std::collections::HashMap::new(),
            requested_method_instantiations: Vec::new(),
            generic_method_sites: std::collections::HashMap::new(),
            requested_class_instantiations: Vec::new(),
            generic_new_sites: HashMap::new(),
            unspecialized_seed_params: HashSet::new(),
            recursive_mixed_methods: HashSet::new(),
            instanceof_tested_params: HashSet::new(),
            callable_return_sigs: HashMap::new(),
            callable_array_return_sigs: HashMap::new(),
            callable_captures: HashMap::new(),
            callable_array_targets: HashMap::new(),
            callable_array_target_versions: HashMap::new(),
            next_callable_array_target_version: 0,
            first_class_callable_targets: HashMap::new(),
            reflection_class_targets: HashMap::new(),
            interfaces: HashMap::new(),
            interface_doc_comments: HashMap::new(),
            classes: crate::fast_hash::FastMap::default(),
            operator_classes: std::cell::Cell::new(None),
            method_owners: std::cell::RefCell::new(None),
            subtype_dispatch_cache: std::cell::RefCell::new((0, Default::default())),
            declared_classes: HashSet::new(),
            declared_class_parents: HashMap::new(),
            declared_class_interfaces: HashMap::new(),
            internal_class_decls: HashSet::new(),
            enums: HashMap::new(),
            declared_interfaces: HashSet::new(),
            declared_traits: HashSet::new(),
            unresolved_catch_types: HashSet::new(),
            declared_trait_methods: HashMap::new(),
            declared_trait_constants: HashMap::new(),
            trait_imported_properties: HashMap::new(),
            trait_imported_constructors: HashSet::new(),
            current_class: None,
            current_method: None,
            current_function: None,
            current_method_is_static: false,
            current_by_ref_return: false,
            closure_depth: 0,
            closure_bound_this_class: None,
            extern_functions: HashMap::new(),
            extern_classes: HashMap::new(),
            packed_classes: HashMap::new(),
            declared_native_records: HashSet::new(),
            extern_globals: HashMap::new(),
            required_libraries: Vec::new(),
            top_level_env: Default::default(),
            active_ref_params: HashSet::new(),
            untyped_ref_params: HashSet::new(),
            active_external_ref_bindings: HashSet::new(),
            active_globals: HashSet::new(),
            // Filled by `check_types_impl` from the whole program before the first walk; an empty
            // set here just means "no `global` declaration is known", which is the safe default
            // for the handful of tests that build a `Checker` directly.
            program_global_names: HashSet::new(),
            active_statics: HashSet::new(),
            mixed_static_locals: HashSet::new(),
            foreach_key_locals: HashSet::new(),
            packed_loop_counter: None,
            eval_barrier_active: false,
            // Filled by `check_types_impl` from the whole program before the first walk, like
            // `program_global_names` above. `false` here means "this program cannot conjure a
            // class at run time", which is the conservative answer for the handful of tests that
            // build a `Checker` directly: it keeps the compile-time diagnostic.
            program_defers_unknown_classes: false,
            program_has_interpreted_callers: false,
            eval_native_callables_reachable: false,
            flow_typed_returns: HashMap::new(),
            family_return_widenings: HashMap::new(),
            method_exists_guards: std::collections::HashSet::new(),
            flow_typed_property_accesses: HashMap::new(),
            null_probe_scope_is_top_level: false,
            pending_null_probe_roots: Vec::new(),
            null_probe_depth: 0,
            break_continue_depth: 0,
            loop_break_envs: Vec::new(),
            finally_break_continue_bases: Vec::new(),
            current_loop_storage_scope: "main".to_string(),
            declared_local_types: HashMap::new(),
            warnings: Vec::new(),
            foreach_warnings: HashMap::new(),
            reference_property_promotions: HashSet::new(),
            reference_property_bindings: Vec::new(),
            clone_override_destinations: Default::default(),
            property_unset_destinations: Default::default(),
            scope_dynamic_mutation_targets: Default::default(),
            throw_access_sites: HashMap::new(),
            binding_position: 0,
            builtin_call_types: HashMap::new(),
            closure_generator_spans: HashSet::new(),
            array_push_property_plans: HashMap::new(),
            member_receiver_types: HashMap::new(),
            member_receiver_analysis_scopes: Vec::new(),
            boxed_reference_outputs: HashMap::new(),
            boxed_reference_promotion_sites: HashMap::new(),
            first_class_builtin_call_types: HashMap::new(),
            loop_storage_types: HashMap::new(),
            loop_storage_scope_spans: HashMap::new(),
            string_incdec_locals: HashSet::new(),
            by_ref_local_storage_types: HashMap::new(),
            ref_param_escaping_callees: HashSet::new(),
            property_bound_ref_params: HashSet::new(),
            property_bound_promotion_sites: HashSet::new(),
            by_ref_call_result_sites: HashSet::new(),
            bcmath_operator_sites: HashSet::new(),
            stdclass_by_ref_walk_sites: HashSet::new(),
            runtime_define_names: HashSet::new(),
            undefined_constant_reads: HashSet::new(),
            widened_ref_params: HashSet::new(),
            widened_ref_param_decls: Vec::new(),
            string_suffix_locals: HashMap::new(),
            dynamic_ref_local_types: HashMap::new(),
            lint_warnings: false,
            strict_locals: false,
            local_conditional_depth: 0,
            local_binding_depth: HashMap::new(),
            narrowed_local_origins: HashMap::new(),
            ref_aliased_locals: HashSet::new(),
            non_builtin_ref_aliased_locals: HashSet::new(),
            ref_assign_aliased_locals: HashSet::new(),
            array_push_capture_sites: HashMap::new(),
            ref_bound_locals: HashSet::new(),
            by_ref_capture_boxed_locals: HashSet::new(),
            boxed_ref_aliased_locals: HashSet::new(),
            conditional_boxed_ref_invalidations: Vec::new(),
            static_local_names: HashSet::new(),
            local_parameter_names: HashSet::new(),
            boxed_parameter_locals: HashSet::new(),
            numeric_region_sites: HashMap::new(),
            numeric_region_retypes: HashSet::new(),
            numeric_region_dependents: HashMap::new(),
            typed_local_names: HashMap::new(),
            local_bind_kill_sites: HashMap::new(),
            local_ref_detach_sites: HashMap::new(),
            retired_ref_detach_sites: HashSet::new(),
            local_retype_sites: HashMap::new(),
            statement_position_expr: None,
            body_contains_eval: false,
            program_contains_eval: false,
            mixed_storage_locals: HashSet::new(),
            retyped_boxed_storage_locals: HashSet::new(),
            guarded_union_types: HashMap::new(),
            mixed_storage_store_sites: HashMap::new(),
            buffer_read_observations: HashMap::new(),
            binding_decision_warnings: HashMap::new(),
            retired_mixed_storage_store_sites: HashSet::new(),
            user_filters_reachable: true,
            user_filter_registered: None,
        }
    }
}
