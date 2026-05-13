//! Purpose:
//! Implements the checker driver init phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Platform;
use crate::types::json_constants::JSON_INT_CONSTANTS;
use crate::types::PhpType;

use super::super::Checker;

impl Checker {
    pub(super) fn new(target_platform: Platform) -> Self {
        let mut constants = HashMap::new();
        constants.insert("PHP_OS".to_string(), PhpType::Str);
        constants.insert("PATHINFO_DIRNAME".to_string(), PhpType::Int);
        constants.insert("PATHINFO_BASENAME".to_string(), PhpType::Int);
        constants.insert("PATHINFO_EXTENSION".to_string(), PhpType::Int);
        constants.insert("PATHINFO_FILENAME".to_string(), PhpType::Int);
        constants.insert("PATHINFO_ALL".to_string(), PhpType::Int);
        constants.insert("FNM_NOESCAPE".to_string(), PhpType::Int);
        constants.insert("FNM_PATHNAME".to_string(), PhpType::Int);
        constants.insert("FNM_PERIOD".to_string(), PhpType::Int);
        constants.insert("FNM_CASEFOLD".to_string(), PhpType::Int);
        constants.insert("STDIN".to_string(), PhpType::stream_resource());
        constants.insert("STDOUT".to_string(), PhpType::stream_resource());
        constants.insert("STDERR".to_string(), PhpType::stream_resource());
        for (name, _value) in JSON_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }

        Self {
            target_platform,
            fn_decls: HashMap::new(),
            function_variant_groups: HashMap::new(),
            functions: HashMap::new(),
            constants,
            closure_return_types: HashMap::new(),
            callable_sigs: HashMap::new(),
            callable_captures: HashMap::new(),
            first_class_callable_targets: HashMap::new(),
            interfaces: HashMap::new(),
            classes: HashMap::new(),
            declared_classes: HashSet::new(),
            enums: HashMap::new(),
            declared_interfaces: HashSet::new(),
            current_class: None,
            current_method: None,
            current_method_is_static: false,
            extern_functions: HashMap::new(),
            extern_classes: HashMap::new(),
            packed_classes: HashMap::new(),
            extern_globals: HashMap::new(),
            required_libraries: Vec::new(),
            top_level_env: HashMap::new(),
            active_ref_params: HashSet::new(),
            active_globals: HashSet::new(),
            active_statics: HashSet::new(),
            break_continue_depth: 0,
            finally_break_continue_bases: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
