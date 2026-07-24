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
use crate::types::array_constants::ARRAY_INT_CONSTANTS;
use crate::types::date_constants::DATE_INT_CONSTANTS;
use crate::types::ent_constants::ENT_INT_CONSTANTS;
use crate::types::error_constants::ERROR_LEVEL_CONSTANTS;
use crate::types::json_constants::JSON_INT_CONSTANTS;
use crate::types::session_constants::SESSION_INT_CONSTANTS;
use crate::types::preg_constants::PREG_INT_CONSTANTS;
use crate::types::stream_constants::STREAM_INT_CONSTANTS;
use crate::types::PhpType;

use super::super::Checker;

impl Checker {
    /// Constructs a new `Checker` with pre-populated builtin constants and empty declaration tables.
    ///
    /// Initializes the global constant map with PHP built-in constants (`PHP_OS`,
    /// `PHP_OS_FAMILY`, `SID`, `PHP_INT_SIZE`, the emulated-PHP version constants
    /// (`PHP_VERSION`, `PHP_MAJOR_VERSION`, `PHP_MINOR_VERSION`, `PHP_RELEASE_VERSION`,
    /// `PHP_VERSION_ID`, `PHP_EXTRA_VERSION`), pathinfo
    /// constants, `ENT_*` HTML-escaping flags, `FNM_*` flags, `STDIN`/`STDOUT`/`STDERR` stream
    /// resources, `LOCK_*` constants), array, JSON, stream, date, preg, session, and error-level
    /// constants. All other declaration tables are initialized empty.
    ///
    /// # Arguments
    /// * `target_platform` - The compilation target platform, stored for use in platform-specific
    ///   type checks and library requirements.
    ///
    /// # Returns
    /// A `Checker` instance ready for the program to be loaded into.
    pub(super) fn new(target_platform: Platform) -> Self {
        let mut constants = HashMap::new();
        constants.insert("PHP_OS".to_string(), PhpType::Str);
        // Deprecated session-id constant; elephc is cookie-only so it always
        // resolves to the empty string (see `codegen::prescan::collect_constants`).
        constants.insert("SID".to_string(), PhpType::Str);
        constants.insert("PHP_EOL".to_string(), PhpType::Str);
        constants.insert("DIRECTORY_SEPARATOR".to_string(), PhpType::Str);
        constants.insert("PATH_SEPARATOR".to_string(), PhpType::Str);
        constants.insert("PHP_OS_FAMILY".to_string(), PhpType::Str);
        constants.insert("PHP_INT_SIZE".to_string(), PhpType::Int);
        constants.insert("PHP_VERSION".to_string(), PhpType::Str);
        constants.insert("PHP_MAJOR_VERSION".to_string(), PhpType::Int);
        constants.insert("PHP_MINOR_VERSION".to_string(), PhpType::Int);
        constants.insert("PHP_RELEASE_VERSION".to_string(), PhpType::Int);
        constants.insert("PHP_VERSION_ID".to_string(), PhpType::Int);
        constants.insert("PHP_EXTRA_VERSION".to_string(), PhpType::Str);
        constants.insert("PATHINFO_DIRNAME".to_string(), PhpType::Int);
        constants.insert("PATHINFO_BASENAME".to_string(), PhpType::Int);
        constants.insert("PATHINFO_EXTENSION".to_string(), PhpType::Int);
        constants.insert("PATHINFO_FILENAME".to_string(), PhpType::Int);
        constants.insert("PATHINFO_ALL".to_string(), PhpType::Int);
        for (name, _value) in ENT_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        constants.insert("FNM_NOESCAPE".to_string(), PhpType::Int);
        constants.insert("FNM_PATHNAME".to_string(), PhpType::Int);
        constants.insert("FNM_PERIOD".to_string(), PhpType::Int);
        constants.insert("FNM_CASEFOLD".to_string(), PhpType::Int);
        constants.insert("STDIN".to_string(), PhpType::stream_resource());
        constants.insert("STDOUT".to_string(), PhpType::stream_resource());
        constants.insert("STDERR".to_string(), PhpType::stream_resource());
        constants.insert("LOCK_SH".to_string(), PhpType::Int);
        constants.insert("LOCK_EX".to_string(), PhpType::Int);
        constants.insert("LOCK_UN".to_string(), PhpType::Int);
        constants.insert("LOCK_NB".to_string(), PhpType::Int);
        for (name, _value) in ARRAY_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        for (name, _value) in JSON_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        for (name, _value) in STREAM_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        for (name, _value) in PREG_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        for (name, _value) in DATE_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        for (name, _value) in SESSION_INT_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        for (name, _value) in ERROR_LEVEL_CONSTANTS {
            constants.insert((*name).to_string(), PhpType::Int);
        }
        // Lexer-tokenized numeric / math constants — needed so `use const PHP_INT_MAX as X`
        // aliases resolve through ConstRef rather than only via dedicated lexer tokens.
        constants.insert("PHP_INT_MAX".to_string(), PhpType::Int);
        constants.insert("PHP_INT_MIN".to_string(), PhpType::Int);
        constants.insert("PHP_FLOAT_MAX".to_string(), PhpType::Float);
        constants.insert("PHP_FLOAT_MIN".to_string(), PhpType::Float);
        constants.insert("PHP_FLOAT_EPSILON".to_string(), PhpType::Float);
        constants.insert("INF".to_string(), PhpType::Float);
        constants.insert("NAN".to_string(), PhpType::Float);
        constants.insert("M_PI".to_string(), PhpType::Float);
        constants.insert("M_E".to_string(), PhpType::Float);
        constants.insert("M_SQRT2".to_string(), PhpType::Float);
        constants.insert("M_PI_2".to_string(), PhpType::Float);
        constants.insert("M_PI_4".to_string(), PhpType::Float);
        constants.insert("M_LOG2E".to_string(), PhpType::Float);
        constants.insert("M_LOG10E".to_string(), PhpType::Float);
        Self {
            target_platform,
            fn_decls: HashMap::new(),
            function_variant_groups: HashMap::new(),
            functions: HashMap::new(),
            resolving_functions: HashSet::new(),
            constants,
            closure_return_types: HashMap::new(),
            callable_sigs: HashMap::new(),
            callable_param_names: HashSet::new(),
            callable_param_sigs: HashMap::new(),
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
            active_statics: HashSet::new(),
            foreach_key_locals: HashSet::new(),
            eval_barrier_active: false,
            break_continue_depth: 0,
            finally_break_continue_bases: Vec::new(),
            warnings: Vec::new(),
            reference_property_promotions: HashSet::new(),
            throw_access_sites: HashMap::new(),
            builtin_call_types: HashMap::new(),
        }
    }
}
