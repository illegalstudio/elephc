//! Purpose:
//! Canonical EIR-consuming assembly backend and public codegen facade.
//! Lowers an EIR `Module` to target assembly while re-exporting shared ABI/runtime helpers.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST-to-EIR lowering and IR optimization.
//!
//! Key details:
//! - EIR is the compiler's codegen contract for emitted user assembly.
//! - `crate::codegen_support` owns shared target, runtime, ABI, and metadata helpers.

mod block_emit;
mod callable_reachability;
pub(crate) mod context;
mod debug_info_adapters;
mod enum_singletons;
mod eval_callable_helpers;
mod eval_class_constant_helpers;
mod eval_constructor_helpers;
mod eval_method_helpers;
mod eval_property_helpers;
mod eval_ref_arg_helpers;
mod eval_reflection_helpers;
mod eval_reflection_owner_helpers;
mod eval_static_property_helpers;
mod fibers;
mod frame;
mod function_variants;
mod literal_defaults;
mod local_analysis;
pub(crate) mod lower_inst;
mod lower_term;
mod runtime_callable_invoker;
mod runtime_metadata;
mod shared_count_guard;
mod shared_helper;
mod shared_mixed_string;
mod shared_state;
mod stack_guard;
mod user_wrapper_adapters;
pub mod value_placement;
mod web;
use runtime_metadata::*;

pub(crate) use crate::codegen_support::collect_constants;
pub use crate::codegen_support::platform;
pub use crate::codegen_support::sentinels::{set_null_repr, NullRepr};
pub(crate) use crate::codegen_support::sentinels::{
    NULL_SENTINEL, UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
};
pub(crate) use crate::codegen_support::{
    abi, bcmath, callable_descriptor, callable_dispatch, callable_invoker_args, cdylib,
    data_section, emit, hash_crypto, interface_wrappers, phar_stream, reflection, runtime,
    sentinels, stream_filters,
    tls, visibility,
};
pub(crate) use crate::codegen_support::{
    autoload_rule_count, compile_php_version, declared_class_names,
    declared_interface_names, declared_trait_names, linked_extensions,
    emit_array_value_type_stamp, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, emit_box_runtime_payload_as_mixed, emit_callback_wrapper,
    emit_extern_callback_trampoline, emit_fiber_wrapper,
    emit_release_pushed_refcounted_temp_after_array_push, emit_write_current_string_stderr,
    emit_write_literal_stderr, runtime_value_tag,
};
#[allow(unused_imports)]
pub use crate::codegen_support::{
    generate_runtime, generate_runtime_with_features, generate_runtime_with_features_pic,
    link_requirements_for_runtime_features, runtime_features_for_program_and_classes,
    LinkRequirement, RuntimeFeatures,
};
pub use crate::codegen_support::{
    prepare_declared_name_order, set_autoload_rule_count, set_compile_profile,
    set_linked_extensions,
};

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::exports::ExportedFunction;
use crate::ir::Module;
use crate::types::PhpType;

/// Output artifact kind selected by the compiler's `--emit` flag.
///
/// `Executable` produces a standalone native binary with a process entry point.
/// `Cdylib` produces a position-independent shared library with exported lifecycle hooks.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Emit {
    Executable,
    Cdylib,
}

/// Compile-time process-isolation model selected for a `--web` executable.
///
/// This value is consumed while emitting the process-entry symbol, so the
/// entry stub references only the requested server entry and does not branch
/// on the isolation model while serving requests.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum WebIsolation {
    /// Run PHP synchronously inside each prefork worker, matching the original server.
    #[default]
    Worker,
    /// Dispatch requests to a persistent supervised pool of handler processes.
    Pool,
    /// Fork one disposable handler process for every request.
    Request,
}

impl WebIsolation {
    /// Returns the bridge C symbol embedded in the generated process-entry stub.
    pub(crate) const fn bridge_symbol(self) -> &'static str {
        match self {
            Self::Worker => "elephc_web_run",
            Self::Pool => "elephc_web_run_pool",
            Self::Request => "elephc_web_run_request",
        }
    }
}

/// Error returned by the Phase 04 IR backend while a required lowering path is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenIrError {
    message: String,
}

impl CodegenIrError {
    /// Creates an error for an EIR shape that is malformed or missing required metadata.
    pub(super) fn invalid_module(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Creates an error for an EIR opcode or backend option not lowered in Phase 04 yet.
    pub(super) fn unsupported(message: impl Into<String>) -> Self {
        Self {
            message: format!("unsupported EIR backend feature: {}", message.into()),
        }
    }

    /// Creates an error for a missing function-local table entry.
    pub(super) fn missing_entry(kind: &str, raw: u32) -> Self {
        Self {
            message: format!("EIR backend missing {} with id {}", kind, raw),
        }
    }
}

impl fmt::Display for CodegenIrError {
    /// Formats the backend error for CLI diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CodegenIrError {}

/// Result type returned by IR backend entry points.
pub type Result<T> = std::result::Result<T, CodegenIrError>;

/// Generates user-code assembly from a lowered EIR module.
///
/// The Phase 04 backend currently supports straight-line scalar main programs and
/// returns explicit unsupported-feature errors for paths that are not lowered yet.
#[allow(dead_code)]
pub fn generate_user_asm_from_ir(
    module: &Module,
    gc_stats: bool,
    heap_debug: bool,
) -> Result<String> {
    let exported_functions: HashMap<String, ExportedFunction> = HashMap::new();
    generate_user_asm_from_ir_with_options(
        module,
        gc_stats,
        heap_debug,
        false,
        Emit::Executable,
        &exported_functions,
        true,
        false,
        WebIsolation::Worker,
    )
}

/// Generates user-code assembly from EIR using the same artifact options as the CLI pipeline.
///
/// `regalloc_linear` selects the linear-scan register allocator; when false the
/// backend keeps every value on the stack (the `--regalloc=stack` fallback).
///
/// `web` restructures the process entry for `--web`: the top-level body becomes
/// the C-callable `_elephc_web_handler` and the real entry point becomes a thin
/// stub that calls the bridge symbol selected by `web_isolation`. When false
/// the entry is byte-for-byte the normal exit-based main.
#[allow(clippy::too_many_arguments)]
pub fn generate_user_asm_from_ir_with_options(
    module: &Module,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
    regalloc_linear: bool,
    web: bool,
    web_isolation: WebIsolation,
) -> Result<String> {
    let mut emitter = match emit {
        Emit::Cdylib => Emitter::new_pic(module.target),
        Emit::Executable => Emitter::new(module.target),
    };
    if module.target.arch == Arch::X86_64 {
        emitter.emit_text_prelude();
    }
    let mut data = DataSection::new();
    block_emit::emit_module(
        module,
        &mut emitter,
        &mut data,
        gc_stats,
        heap_debug,
        requires_elephc_tls,
        emit,
        regalloc_linear,
        web,
        web_isolation,
    )?;
    Ok(finalize_user_asm(
        module,
        emitter,
        data,
        emit,
        exported_functions,
    ))
}

/// Appends literal data and the minimal user-runtime metadata needed by linked helpers.
fn finalize_user_asm(
    module: &Module,
    mut emitter: Emitter,
    mut data: DataSection,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
) -> String {
    let eval_bridge = module.required_runtime_features.eval_bridge;
    let emit_eval_reflection_metadata =
        eval_bridge || module.required_runtime_features.eval_scope;
    if eval_bridge {
        eval_property_helpers::emit_eval_property_helpers(module, &mut emitter, &mut data);
        eval_static_property_helpers::emit_eval_static_property_helpers(
            module,
            &mut emitter,
            &mut data,
        );
        eval_class_constant_helpers::emit_eval_class_constant_helpers(
            module,
            &mut emitter,
            &mut data,
        );
    }
    let eval_callable_support_needed =
        eval_bridge && eval_callable_helpers::module_needs_eval_callable_descriptor_support(module);
    let eval_callable_support = eval_callable_helpers::emit_eval_callable_descriptor_support(
        module,
        &mut emitter,
        &mut data,
        eval_callable_support_needed,
    );
    if eval_bridge {
        eval_constructor_helpers::emit_eval_constructor_helpers(
            module,
            &mut emitter,
            &mut data,
            &eval_callable_support,
        );
        eval_method_helpers::emit_eval_method_helpers(
            module,
            &mut emitter,
            &mut data,
            &eval_callable_support,
        );
        eval_reflection_helpers::emit_eval_reflection_helpers(module, &mut emitter);
        eval_reflection_owner_helpers::emit_eval_reflection_owner_helpers(module, &mut emitter);
    }
    let empty_globals = HashSet::<String>::new();
    let empty_static_vars = HashMap::<(String, String), PhpType>::new();
    let user_functions = runtime_user_function_sigs(module);
    let function_variant_groups = runtime_function_variant_groups(module);
    let mut allowed_class_names = runtime_referenced_class_names(module);
    if module_uses_dynamic_callable_lookup(module)
        || module_uses_unserialize(module)
        || module.required_runtime_features.eval_bridge
    {
        allowed_class_names.extend(module.class_infos.keys().cloned());
    }
    let runtime_interfaces = runtime_referenced_interfaces(module, &allowed_class_names);
    let runtime_classes = runtime_class_infos(module);
    crate::codegen::interface_wrappers::emit_interface_return_wrappers(
        &mut emitter,
        &runtime_interfaces,
        &runtime_classes,
        Some(&allowed_class_names),
    );
    emit_intrinsic_method_wrappers(module, &mut emitter);
    debug_info_adapters::emit_debug_info_adapters(module, &runtime_classes, &mut emitter);
    user_wrapper_adapters::emit_user_wrapper_adapters(
        module,
        &runtime_classes,
        &mut emitter,
        &mut data,
    );
    if matches!(emit, Emit::Cdylib) {
        let mut sorted_exports: Vec<&ExportedFunction> = exported_functions.values().collect();
        sorted_exports.sort_by(|a, b| a.name.cmp(&b.name));
        crate::codegen::cdylib::emit_cdylib_exports(&mut emitter, module.target, &sorted_exports);
    }
    let user_data = runtime::emit_runtime_data_user(
        &empty_globals,
        &empty_static_vars,
        &user_functions,
        &function_variant_groups,
        &runtime_interfaces,
        &module.declared_interface_names,
        &module.trait_table.names,
        &module.declared_trait_uses,
        &module.declared_trait_source_lines,
        &runtime_classes,
        &module.enum_infos,
        Some(&allowed_class_names),
        module.required_runtime_features.dom_bridge,
        emit_eval_reflection_metadata,
        // The source path now feeds `Throwable::getFile()` and the ` in <file>:<line>`
        // suffix of the uncaught-exception report as well as the eval Reflection
        // source-location hooks, so it is passed unconditionally. It is not new
        // information in the artifact: `__FILE__` already bakes this exact
        // canonicalized string into any program that mentions it, and reference PHP
        // prints it in every fatal error.
        module.source_path.as_deref(),
        module.target,
    );
    let data_output = data.emit(module.target);

    let mut user_asm = emitter.output();
    if !data_output.is_empty() {
        user_asm.push('\n');
        user_asm.push_str(&data_output);
    }
    user_asm.push('\n');
    user_asm.push_str(&user_data);
    let mut exported: HashSet<String> = exported_functions
        .values()
        .map(|export| module.target.extern_symbol(&export.name))
        .collect();
    match emit {
        Emit::Cdylib => {
            for lifecycle in [
                "elephc_init",
                "elephc_shutdown",
                "elephc_last_error",
                "elephc_free",
            ] {
                exported.insert(module.target.extern_symbol(lifecycle));
            }
        }
        // An executable exports only its entry point. Everything else is `.globl` purely so the
        // two objects can find each other, and a `.globl` is an export — hence a dead-strip root,
        // which is why unreferenced per-class machinery survived stripping.
        Emit::Executable => {
            exported.insert(module.target.extern_symbol("main"));
        }
    }
    crate::codegen::visibility::append_hidden_directives(
        &user_asm,
        &exported,
        module.target.platform,
    )
}
