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
use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

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
        let mut constants = HashMap::default();
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
            fn_decls: HashMap::default(),
            folded_function_names: Default::default(),
            canonical_function_names: Default::default(),
            function_variant_groups: HashMap::default(),
            failed_variant_contracts: HashMap::default(),
            completed_function_signatures: HashSet::default(),
            functions: HashMap::default(),
            resolving_functions: HashSet::default(),
            recursive_param_widenings: HashMap::default(),
            functions_called_directly: HashSet::default(),
            constants,
            closure_return_types: HashMap::default(),
            callable_sigs: HashMap::default(),
            static_property_callable_sigs: HashMap::default(),
            callable_param_names: HashSet::default(),
            callable_param_sigs: HashMap::default(),
            callable_param_site_sigs: HashMap::default(),
            strict_types: false,
            internal_callback_binding: false,
            param_specialization_seen: HashSet::default(),
            class_children_index: None,
            method_impl_users_index: None,
            property_sharers_index: None,
            interface_method_boxed_params: HashSet::default(),
            requested_instantiations: Vec::new(),
            generic_call_sites: HashMap::default(),
            class_templates: Vec::new(),
            method_templates: crate::fast_hash::FastMap::default(),
            requested_method_instantiations: Vec::new(),
            generic_method_sites: crate::fast_hash::FastMap::default(),
            requested_class_instantiations: Vec::new(),
            generic_new_sites: HashMap::default(),
            unspecialized_seed_params: HashSet::default(),
            recursive_mixed_methods: HashSet::default(),
            instanceof_tested_params: HashSet::default(),
            callable_return_sigs: HashMap::default(),
            callable_array_return_sigs: HashMap::default(),
            callable_captures: HashMap::default(),
            callable_array_targets: HashMap::default(),
            callable_array_target_versions: HashMap::default(),
            next_callable_array_target_version: 0,
            first_class_callable_targets: HashMap::default(),
            reflection_class_targets: HashMap::default(),
            interfaces: HashMap::default(),
            interface_doc_comments: HashMap::default(),
            classes: crate::fast_hash::FastMap::default(),
            operator_classes: std::cell::Cell::new(None),
            method_owners: std::cell::RefCell::new(None),
            subtype_dispatch_cache: std::cell::RefCell::new((0, Default::default())),
            descendant_classes_cache: std::cell::RefCell::new((0, Default::default())),
            interface_implementers_cache: std::cell::RefCell::new((0, Default::default())),
            declared_classes: HashSet::default(),
            declared_class_parents: HashMap::default(),
            declared_class_interfaces: HashMap::default(),
            internal_class_decls: HashSet::default(),
            enums: HashMap::default(),
            declared_interfaces: HashSet::default(),
            declared_traits: HashSet::default(),
            unresolved_catch_types: HashSet::default(),
            declared_trait_methods: HashMap::default(),
            declared_trait_constants: HashMap::default(),
            trait_imported_properties: HashMap::default(),
            trait_imported_constructors: HashSet::default(),
            current_class: None,
            current_method: None,
            current_function: None,
            current_method_is_static: false,
            current_by_ref_return: false,
            closure_depth: 0,
            closure_bound_this_class: None,
            extern_functions: HashMap::default(),
            extern_classes: HashMap::default(),
            packed_classes: HashMap::default(),
            declared_native_records: HashSet::default(),
            extern_globals: HashMap::default(),
            required_libraries: Vec::new(),
            top_level_env: Default::default(),
            active_ref_params: HashSet::default(),
            untyped_ref_params: HashSet::default(),
            active_external_ref_bindings: HashSet::default(),
            active_globals: HashSet::default(),
            // Filled by `check_types_impl` from the whole program before the first walk; an empty
            // set here just means "no `global` declaration is known", which is the safe default
            // for the handful of tests that build a `Checker` directly.
            program_global_names: HashSet::default(),
            active_statics: HashSet::default(),
            mixed_static_locals: HashSet::default(),
            foreach_key_locals: HashSet::default(),
            packed_loop_counter: None,
            eval_barrier_active: false,
            // Filled by `check_types_impl` from the whole program before the first walk, like
            // `program_global_names` above. `false` here means "this program cannot conjure a
            // class at run time", which is the conservative answer for the handful of tests that
            // build a `Checker` directly: it keeps the compile-time diagnostic.
            program_defers_unknown_classes: false,
            program_has_interpreted_callers: false,
            eval_native_callables_reachable: false,
            flow_typed_returns: HashMap::default(),
            family_return_widenings: HashMap::default(),
            method_exists_guards: crate::fast_hash::FastSet::default(),
            flow_typed_property_accesses: HashMap::default(),
            null_probe_scope_is_top_level: false,
            pending_null_probe_roots: Vec::new(),
            null_probe_depth: 0,
            break_continue_depth: 0,
            loop_break_envs: Vec::new(),
            finally_break_continue_bases: Vec::new(),
            current_loop_storage_scope: "main".to_string(),
            declared_local_types: HashMap::default(),
            warnings: Vec::new(),
            foreach_warnings: HashMap::default(),
            reference_property_promotions: HashSet::default(),
            reference_property_bindings: Vec::new(),
            clone_override_destinations: Default::default(),
            property_unset_destinations: Default::default(),
            method_worklist: Default::default(),
            scope_dynamic_mutation_targets: Default::default(),
            throw_access_sites: HashMap::default(),
            binding_position: 0,
            builtin_call_types: HashMap::default(),
            closure_generator_spans: HashSet::default(),
            array_push_property_plans: HashMap::default(),
            member_receiver_types: HashMap::default(),
            member_receiver_analysis_scopes: Vec::new(),
            boxed_reference_outputs: HashMap::default(),
            boxed_reference_promotion_sites: HashMap::default(),
            first_class_builtin_call_types: HashMap::default(),
            loop_storage_types: HashMap::default(),
            loop_storage_scope_spans: HashMap::default(),
            string_incdec_locals: HashSet::default(),
            by_ref_local_storage_types: HashMap::default(),
            ref_param_escaping_callees: HashSet::default(),
            property_bound_ref_params: HashSet::default(),
            property_bound_promotion_sites: HashSet::default(),
            by_ref_call_result_sites: HashSet::default(),
            bcmath_operator_sites: HashSet::default(),
            stdclass_by_ref_walk_sites: HashSet::default(),
            runtime_define_names: HashSet::default(),
            undefined_constant_reads: HashSet::default(),
            widened_ref_params: HashSet::default(),
            widened_ref_param_decls: Vec::new(),
            string_suffix_locals: HashMap::default(),
            dynamic_ref_local_types: HashMap::default(),
            lint_warnings: false,
            strict_locals: false,
            local_conditional_depth: 0,
            local_binding_depth: HashMap::default(),
            narrowed_local_origins: HashMap::default(),
            ref_aliased_locals: HashSet::default(),
            non_builtin_ref_aliased_locals: HashSet::default(),
            ref_assign_aliased_locals: HashSet::default(),
            array_push_capture_sites: HashMap::default(),
            ref_bound_locals: HashSet::default(),
            by_ref_capture_boxed_locals: HashSet::default(),
            boxed_ref_aliased_locals: HashSet::default(),
            conditional_boxed_ref_invalidations: Vec::new(),
            static_local_names: HashSet::default(),
            local_parameter_names: HashSet::default(),
            boxed_parameter_locals: HashSet::default(),
            numeric_region_sites: HashMap::default(),
            numeric_region_retypes: HashSet::default(),
            numeric_region_dependents: HashMap::default(),
            typed_local_names: HashMap::default(),
            local_bind_kill_sites: HashMap::default(),
            local_ref_detach_sites: HashMap::default(),
            retired_ref_detach_sites: HashSet::default(),
            local_retype_sites: HashMap::default(),
            statement_position_expr: None,
            body_contains_eval: false,
            program_contains_eval: false,
            mixed_storage_locals: HashSet::default(),
            retyped_boxed_storage_locals: HashSet::default(),
            guarded_union_types: HashMap::default(),
            mixed_storage_store_sites: HashMap::default(),
            buffer_read_observations: HashMap::default(),
            binding_decision_warnings: HashMap::default(),
            retired_mixed_storage_store_sites: HashSet::default(),
            user_filters_reachable: true,
            user_filter_registered: None,
        }
    }
}
