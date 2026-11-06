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

mod aarch64_relax;
mod user_assembly;
pub use user_assembly::UserAssembly;
mod block_emit;
pub(crate) use block_emit::static_property_owner_body;
pub use block_emit::print_op_stats;
mod classlike_activation;
mod source_units;
mod block_layout;
pub use block_emit::generic_body_sharing_oracle;
pub(crate) mod callable_reachability;
mod callable_signatures;
mod const_default_values;
mod constant_materialization;
pub(crate) mod context;
#[cfg(test)]
pub(crate) use context::NUMERIC_FALLBACK_QUERIES;
#[cfg(test)]
pub(crate) use context::LOCAL_STORE_SCANNED_INSTRUCTIONS;
mod enum_singletons;
mod eval_boxed_callable_helper;
mod eval_callable_helpers;
mod eval_class_constant_helpers;
mod class_id_dispatch;
mod eval_argument_helpers;
mod eval_constructor_helpers;
mod eval_error_handler_helpers;
mod eval_handler_helpers;
mod eval_inventory_helper;
mod eval_method_helpers;
mod eval_value_helpers;
mod eval_property_helpers;
mod eval_ref_arg_helpers;
mod eval_reflection_helpers;
mod eval_reflection_owner_helpers;
mod eval_static_property_helpers;
mod eval_status_helper;
mod fibers;
mod frame;
mod function_variants;
mod literal_defaults;
mod literal_method_index;
mod local_analysis;
mod mbstring_configuration;
pub(crate) mod lower_inst;
#[cfg(test)]
pub(crate) use lower_inst::callables::INSTANCE_TARGET_BUILDS;
mod lower_term;
mod machine_peephole;
mod parallel_moves;
mod runtime_callable_invoker;
#[cfg(test)]
pub(crate) use runtime_callable_invoker::function_returns_owned_string;
mod runtime_metadata;
mod return_ownership;
mod shared_count_guard;
mod shared_helper;
mod shared_mixed_callable;
mod shared_mixed_string;
mod shared_null_method_trace;
mod shared_static_throw;
mod shared_type_error_given;
mod shared_eval_status;
mod shared_reflection;
mod shared_mbstring_callable;
mod shared_state;
pub(crate) mod stack_guard;
mod stack_numeric;
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
    curl, data_section, emit, hash_crypto, iconv_bridge, interface_wrappers, phar_stream,
    random, reflection, runtime,
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
    emit_write_literal_stderr, runtime_value_tag, container_value_tag,
};
#[allow(unused_imports)]
pub use crate::codegen_support::{
    generate_runtime, generate_runtime_with_features, generate_runtime_with_features_mode,
    generate_runtime_with_features_pic, link_requirements_for_runtime_features,
    runtime_features_for_program_and_classes, LinkRequirement, RuntimeFeatures,
};
pub use crate::codegen_support::{
    prepare_declared_name_order, set_autoload_rule_count, set_compile_profile,
    set_ini_overrides, set_linked_extensions, set_opcache_api_restricted,
    set_opcache_manifest_len,
};

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::exports::ExportedFunction;
use crate::ir::Module;
use crate::names::php_symbol_key;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Which PHP functions carry `--instrument` hooks.
///
/// Instrumenting everything is exact but costs two clock reads and a bookkeeping
/// update on every call, which is why it is a dev-build tool. Instrumenting a
/// chosen few keeps that exactness where it was asked for and leaves the rest of
/// the program at full speed — the shape production tracers use.
///
/// The trade is real and is reported rather than hidden: with a partial set, an
/// uninstrumented callee's time lands in its instrumented caller's SELF, so self
/// values stop partitioning the root's inclusive. The runtime is told, and says so.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Instrumentation {
    /// No hooks at all.
    #[default]
    Off,
    /// The `{main}` root and every non-synthetic PHP function.
    All,
    /// Only the named functions. `{main}` selects the top-level root; a trailing
    /// `*` matches by prefix, so `PDOStatement::*` covers a class.
    Only(Vec<String>),
}

impl Instrumentation {
    /// Whether any hook is emitted at all.
    pub fn is_on(&self) -> bool {
        !matches!(self, Instrumentation::Off)
    }

    /// Whether this function should carry hooks.
    pub fn covers(&self, name: &str) -> bool {
        match self {
            Instrumentation::Off => false,
            Instrumentation::All => true,
            Instrumentation::Only(names) => names.iter().any(|pattern| {
                match pattern.strip_suffix('*') {
                    Some(prefix) => name.starts_with(prefix),
                    None => name == pattern,
                }
            }),
        }
    }

    /// Whether the set is a subset, which is what makes the numbers need a caveat.
    pub fn is_partial(&self) -> bool {
        matches!(self, Instrumentation::Only(_))
    }
}



/// Output artifact kind selected by the compiler's `--emit` flag.
///
/// `Executable` produces a standalone native binary with a process entry point.
/// `Cdylib` produces a position-independent shared library with exported lifecycle hooks.
/// `Staticlib` produces an `ar` archive of the same exported surface, for a host
/// that links elephc into its own binary — an Xcode project, say — instead of
/// loading it at run time.
///
/// `Staticlib` is *not* PIC. `Emitter::new_pic` exists for dynamic loading,
/// where the loader must resolve cross-object references at `dlopen` time; its
/// GOT indirection is unrelated to position independence as such. An archive is
/// merged once into the host's final binary by the host's own linker, exactly
/// like the executable path, whose non-PIC output is already PC-relative and
/// already yields PIE binaries.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Emit {
    Executable,
    Cdylib,
    Staticlib,
    /// A real PHP module with host-owned Zend registration and request snapshots.
    Phpext,
}

impl Emit {
    /// Returns whether this artifact is a library exposing `#[Export]`
    /// trampolines and lifecycle symbols rather than a process entry point.
    pub fn is_library(self) -> bool {
        matches!(self, Emit::Cdylib | Emit::Staticlib | Emit::Phpext)
    }
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
    /// Optional source location attached by the block walker.
    location: Option<String>,
    /// Set only by the codegen worker's deferral watch: not a fault, a request to re-emit this
    /// body on the serial pass. It survives `at()` because the abort happens inside a block.
    deferred: bool,
    /// Every body the backend inventory could not lower, by EIR function name. Empty unless the
    /// inventory ran (`ELEPHC_BACKEND_INVENTORY=1` or [`collect_backend_refusals`]).
    failed_bodies: Vec<(String, String)>,
    /// The deferrable unit this refusal belongs to when it is not the body being emitted.
    ///
    /// Static-property defaults are written by the entry program's prologue, so a refused
    /// initializer surfaced as a `main` failure, which no runtime autoloader can supply, and the
    /// whole build stopped. Naming the property (`Class::$prop`) lets the inventory record the
    /// refusal against the declaring class's file, which the deferral round hands back.
    owner_body: Option<String>,
}

/// Whether the backend scans every body and reports all refusals together, set by the pipeline
/// when refused bodies can be handed to the runtime (see `pipeline::backend`).
static COLLECT_BACKEND_REFUSALS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Makes the backend collect every refused body instead of stopping at the first one.
pub(crate) fn collect_backend_refusals(enabled: bool) {
    COLLECT_BACKEND_REFUSALS.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Returns whether the pipeline asked the backend to collect every refused body.
pub(crate) fn collects_backend_refusals() -> bool {
    COLLECT_BACKEND_REFUSALS.load(std::sync::atomic::Ordering::Relaxed)
}

impl CodegenIrError {
    /// Creates an error for a failed file operation of the code generator itself.
    fn io(context: &str, error: std::io::Error) -> Self {
        Self {
            message: format!("{context}: {error}"),
            location: None,
            deferred: false,
            failed_bodies: Vec::new(),
            owner_body: None,
        }
    }

    /// Creates an error for an EIR shape that is malformed or missing required metadata.
    pub(super) fn invalid_module(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: None,
            deferred: false,
            failed_bodies: Vec::new(),
            owner_body: None,
        }
    }

    /// Creates an error for an EIR opcode or backend option not lowered in Phase 04 yet.
    pub(super) fn unsupported(message: impl Into<String>) -> Self {
        Self {
            message: format!("unsupported EIR backend feature: {}", message.into()),
            location: None,
            deferred: false,
            failed_bodies: Vec::new(),
            owner_body: None,
        }
    }

    /// Creates an error for a missing function-local table entry.
    pub(super) fn missing_entry(kind: &str, raw: u32) -> Self {
        Self {
            message: format!("EIR backend missing {} with id {}", kind, raw),
            location: None,
            deferred: false,
            failed_bodies: Vec::new(),
            owner_body: None,
        }
    }

    /// Attaches the innermost known source location to this backend error.
    pub(super) fn at(mut self, location: impl Into<String>) -> Self {
        self.location.get_or_insert_with(|| location.into());
        self
    }

    /// Returns the location-free cause used to group inventory failures.
    pub(super) fn message(&self) -> &str {
        &self.message
    }

    /// Stops a body a codegen worker is going to discard anyway.
    ///
    /// A worker cannot emit a body that reaches a shared cache, because the helper behind that
    /// cache belongs to the module once and the worker cannot know whether another worker is
    /// emitting it too. That was discovered by emitting the WHOLE body and then rolling it back:
    /// on the Symfony module, 2 101 439 143 of 2 398 603 000 bytes — 87.6% — were emitted twice,
    /// once into a worker that threw them away and once into the serial pass. Stopping at the
    /// instruction that touched the cache leaves the same set of bodies deferred and throws away
    /// a prefix instead of a body.
    pub(super) fn deferred_body() -> Self {
        Self {
            message: "codegen body deferred to the serial pass".to_string(),
            location: None,
            deferred: true,
            failed_bodies: Vec::new(),
            owner_body: None,
        }
    }

    /// Returns whether this is the deferral signal rather than a backend fault.
    pub(super) fn is_deferred_body(&self) -> bool {
        self.deferred
    }

    /// Attributes this refusal to a deferrable unit other than the body being emitted.
    pub(super) fn owned_by(mut self, owner_body: impl Into<String>) -> Self {
        self.owner_body.get_or_insert_with(|| owner_body.into());
        self
    }

    /// Returns the unit the inventory records this refusal against, when not the emitted body.
    pub(super) fn owner_body(&self) -> Option<&str> {
        self.owner_body.as_deref()
    }

    /// Returns the EIR function names the backend inventory could not lower.
    pub(crate) fn failed_bodies(&self) -> &[(String, String)] {
        &self.failed_bodies
    }
}

impl fmt::Display for CodegenIrError {
    /// Formats the backend error for CLI diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)?;
        match &self.location {
            Some(location) => write!(formatter, " ({})", location),
            None => Ok(()),
        }
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
        false, // counters
        Instrumentation::Off, // instrument
        false, // probe
        heap_debug,
        false,
        Emit::Executable,
        &exported_functions,
        true,
        false,
        WebIsolation::Worker,
        None,
    )
    .map(UserAssembly::into_string)
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
    counters: bool,
    instrument: Instrumentation,
    probe: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
    regalloc_linear: bool,
    web: bool,
    web_isolation: WebIsolation,
    spill_path: Option<&std::path::Path>,
) -> Result<UserAssembly> {
    generate_user_asm_from_ir_with_options_and_features(
        module,
        gc_stats,
        counters,
        instrument,
        probe,
        heap_debug,
        requires_elephc_tls,
        emit,
        exported_functions,
        regalloc_linear,
        web,
        web_isolation,
        spill_path,
    )
    .map(|output| output.asm)
}

/// User assembly together with runtime dependencies introduced by synthetic codegen wrappers.
pub struct GeneratedUserAsm {
    /// Complete target assembly for the user object (possibly spilled to disk while emitting).
    pub asm: UserAssembly,
    /// Whether generated wrappers reference native error or exception handler state.
    pub handler_state: bool,
    /// Whether generated wrappers reference the native boxed object clone adapter.
    pub object_clone: bool,
}

/// Generates user assembly while preserving dependencies discovered after module EIR lowering.
#[allow(clippy::too_many_arguments)]
pub fn generate_user_asm_from_ir_with_options_and_features(
    module: &Module,
    gc_stats: bool,
    counters: bool,
    instrument: Instrumentation,
    probe: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
    regalloc_linear: bool,
    web: bool,
    web_isolation: WebIsolation,
    spill_path: Option<&std::path::Path>,
) -> Result<GeneratedUserAsm> {
    let mut emitter = match emit {
        Emit::Cdylib | Emit::Phpext => Emitter::new_cdylib(module.target),
        // A staticlib joins the executable path: it is linked once into the host
        // binary, so it needs no GOT indirection, but it still exposes the same
        // recoverable host boundary as a cdylib.
        Emit::Staticlib => Emitter::new_staticlib(module.target),
        Emit::Executable => Emitter::new(module.target),
    };
    if let Some(path) = spill_path {
        emitter
            .enable_spill(path)
            .map_err(|error| CodegenIrError::io("cannot create the assembly spill file", error))?;
    }
    // User bodies can contain host-boundary notifications in addition to the separately
    // generated runtime object, so they need the output profile before EIR lowering begins.
    emitter.phpext_output = emit == Emit::Phpext;
    if module.target.arch == Arch::X86_64 {
        emitter.emit_text_prelude();
    }
    let mut data = DataSection::new();
    let shared = block_emit::emit_module(
        module,
        &mut emitter,
        &mut data,
        gc_stats,
        counters,
        instrument,
        probe,
        heap_debug,
        requires_elephc_tls,
        emit,
        regalloc_linear,
        web,
        web_isolation,
    )?;
    finalize_user_asm(
        module,
        emitter,
        data,
        emit,
        exported_functions,
        heap_debug,
        shared,
        regalloc_linear,
    )
}

/// Appends literal data and the minimal user-runtime metadata needed by linked helpers.
fn finalize_user_asm(
    module: &Module,
    mut emitter: Emitter,
    mut data: DataSection,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
    heap_debug: bool,
    mut shared: shared_state::SharedCodegenState,
    regalloc_linear: bool,
) -> Result<GeneratedUserAsm> {
    let eval_bridge = module.required_runtime_features.eval_bridge;
    mbstring_configuration::emit(module, &mut emitter, &mut data);
    let emit_eval_reflection_metadata =
        eval_bridge || module.required_runtime_features.eval_scope;
    if eval_bridge {
        eval_static_property_helpers::emit_eval_static_property_helpers(
            module,
            &mut emitter,
            &mut data,
        );
        // The catalog is registered by ONE module-level helper, and it is emitted
        // BEFORE the callable-argument-normalizer gate below, not after it:
        // registering a native free function can REQUIRE that normalizer
        // (`emit_eval_native_function_invoker_inline` raises
        // `shared.callable_argument_normalizer`), and that gate is the flag's
        // only reader. Emitting it afterwards would leave the flag unread and
        // the program would call a normalizer that was never emitted.
        eval_inventory_helper::emit_eval_inventory_helper(
            module,
            &mut emitter,
            &mut data,
            &mut shared,
            regalloc_linear,
        )?;
    }
    eval_class_constant_helpers::emit_eval_class_constant_helpers(module, &mut emitter, &mut data);
    let eval_callable_support_needed =
        eval_bridge && eval_callable_helpers::module_needs_eval_callable_descriptor_support(module);
    let eval_callable_support = eval_callable_helpers::emit_eval_callable_descriptor_support(
        module,
        &mut emitter,
        &mut data,
        eval_callable_support_needed,
        eval_bridge,
    );
    if eval_bridge {
        eval_boxed_callable_helper::emit_eval_boxed_callable_descriptor_helper(&mut emitter, &mut data);
        // After the callable support: a `\Closure`-typed property slot converts the boxed eval
        // callback into a descriptor with the same cast a `callable` parameter uses.
        eval_property_helpers::emit_eval_property_helpers(
            module,
            &mut emitter,
            &mut data,
            &eval_callable_support,
        );
        eval_error_handler_helpers::emit_eval_error_handler_helpers(
            module,
            &mut emitter,
            &eval_callable_support,
        );
        eval_handler_helpers::emit_eval_exception_handler_helpers(
            module,
            &mut emitter,
            &eval_callable_support,
        );
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
    let table_reg = runtime_callable_invoker::invoker_table_reg(emitter.target.arch);
    lower_inst::emit_declared_parameter_refusal(
        &mut emitter,
        &mut data,
        runtime_callable_invoker::INVOKER_DESCRIPTOR_OFFSET,
        runtime_callable_invoker::INVOKER_TABLE_OFFSET,
        table_reg,
        module.required_runtime_features.eval_bridge || frame::module_materializes_backtrace(module),
    );
    if shared.callable_argument_normalizer || eval_callable_support.argument_normalizer_needed {
        lower_inst::emit_callable_argument_normalizer(module, &mut emitter, &mut data, &mut shared)?;
    }
    // The eval status dispatch is the same at every call site, so it is one
    // module-level helper too. Emitted OUTSIDE the bridge gate: a literal-eval
    // program uses the eval SCOPE accessors without linking the bridge, and
    // those report statuses through the same helper.
    eval_status_helper::emit_eval_status_helper(
        module,
        &mut emitter,
        &mut data,
        &mut shared,
        regalloc_linear,
    )?;
    let empty_globals = HashSet::<String>::new();
    let empty_static_vars = HashMap::<(String, String), PhpType>::new();
    let user_functions = runtime_user_function_sigs(module);
    let function_variant_groups = runtime_function_variant_groups(module);
    let mut allowed_class_names = runtime_referenced_class_names(module);
    // `get_class_methods()` resolves a class NAME through `__rt_class_id_by_name`, which scans
    // `_classes_by_name`; a class the program only names there (`get_class_methods(C::class)`)
    // is otherwise trimmed from that table and the call answered php's TypeError.
    if module_uses_dynamic_callable_lookup(module)
        || module_uses_unserialize(module)
        || module.required_runtime_features.eval_bridge
        || module.required_runtime_features.class_methods_introspection
    {
        allowed_class_names.extend(module.class_infos.keys().cloned());
    }
    let runtime_interfaces = runtime_referenced_interfaces(module, &allowed_class_names);
    let runtime_classes = runtime_class_infos(module);
    crate::codegen_support::source_method_adapters::emit_source_method_adapters(
        &mut emitter,
        &runtime_classes,
        Some(&allowed_class_names),
    )
    .map_err(CodegenIrError::invalid_module)?;
    crate::codegen::interface_wrappers::emit_interface_return_wrappers(
        &mut emitter,
        &runtime_interfaces,
        &runtime_classes,
        Some(&allowed_class_names),
    )
    .map_err(CodegenIrError::invalid_module)?;
    emit_intrinsic_method_wrappers(module, &mut emitter);
    if emit.is_library() {
        let mut sorted_exports: Vec<&ExportedFunction> = exported_functions.values().collect();
        sorted_exports.sort_by(|a, b| a.c_name.cmp(&b.c_name));
        let emit_exports = if emit == Emit::Phpext {
            crate::codegen::cdylib::emit_phpext_exports
        } else { crate::codegen::cdylib::emit_cdylib_exports };
        emit_exports(
            &mut emitter,
            &mut data,
            module.target,
            &sorted_exports,
            heap_debug,
            module.mbstring_startup.as_ref().map(|_| "__rt_mbstring_startup_status"),
        );
    }
    let property_initializer_ids = module.functions.iter()
        .filter(|function| function.flags.is_synthetic)
        .filter_map(|function| function.name.strip_prefix("_class_propinit_")?.parse::<u64>().ok())
        .collect();
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
        &module.declared_class_source_files,
        &runtime_classes,
        &module.enum_infos,
        &property_initializer_ids,
        Some(&allowed_class_names),
        emit_eval_reflection_metadata,
        // The source path now feeds `Throwable::getFile()` and the ` in <file>:<line>`
        // suffix of the uncaught-exception report as well as the eval Reflection
        // source-location hooks, so it is passed unconditionally. It is not new
        // information in the artifact: `__FILE__` already bakes this exact
        // canonicalized string into any program that mentions it, and reference PHP
        // prints it in every fatal error.
        module.source_path.as_deref(),
        module.target,
    )
    .map_err(CodegenIrError::invalid_module)?;

    let activation_names = classlike_activation::collect_activation_registry_names(module);
    if module.required_runtime_features.class_introspection {
        for name in &activation_names.interfaces {
            data.add_comm(
                crate::names::classlike_activation_symbol(
                    crate::parser::ast::ClassLikeKind::Interface,
                    name,
                ),
                8,
            );
        }
    }

    let data_output = data.emit(module.target);
    // The emitted text is the whole program (1.5 GB on the Symfony build). Everything below is
    // appended to a separate, small tail: pushing it onto the text reallocated that buffer, and
    // the realloc briefly held two copies of it.
    let (spilled, text) = emitter
        .finish()
        .map_err(|error| CodegenIrError::io("cannot spill the user assembly to disk", error))?;
    let mut user_asm = String::new();
    if !data_output.is_empty() {
        user_asm.push('\n');
        user_asm.push_str(&data_output);
    }
    user_asm.push('\n');
    user_asm.push_str(&user_data);
    if module.required_runtime_features.const_introspection {
        let const_registry = scalar_const_registry(module);
        let mut enum_names: Vec<String> = module
            .enum_infos
            .keys()
            .map(|name| name.trim_start_matches('\\').to_string())
            .collect();
        enum_names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let error_class_id = runtime_classes
            .get("Error")
            .map(|info| info.class_id)
            .unwrap_or(u64::MAX);
        user_asm.push('\n');
        user_asm.push_str(&runtime::emit_const_registry_data(
            &const_registry,
            &enum_names,
            error_class_id,
        ));
    }
    if module.required_runtime_features.class_introspection {
        let class_names = sorted_lowercased_registry_names(
            module
                .class_infos
                .keys()
                .filter(|name| !is_internal_synthetic_class_name(name)),
        );
        let interface_names = sorted_lowercased_registry_names(module.interface_infos.keys());
        let trait_names = sorted_lowercased_registry_names(module.trait_table.names.iter());
        user_asm.push('\n');
        user_asm.push_str(&runtime::emit_class_registry_data(
            &class_names,
            &interface_names,
            &trait_names,
            &activation_names.interfaces,
        ));
    } else if module.required_runtime_features.eval_bridge {
        // Eval-owned interface declarations are visible through the same
        // request-active registry as AOT declarations. The eval bridge can
        // call its interface lookup helper without an AOT class-introspection
        // expression; emit only the two tables that helper needs.
        let interface_names = sorted_lowercased_registry_names(module.interface_infos.keys());
        user_asm.push('\n');
        user_asm.push_str(&runtime::emit_interface_registry_data(
            &interface_names,
            &activation_names.interfaces,
        ));
    }
    if module.required_runtime_features.class_relation_introspection {
        user_asm.push('\n');
        user_asm.push_str(&runtime::emit_class_relation_registry_data(module));
    }
    if module.required_runtime_features.class_methods_introspection {
        user_asm.push('\n');
        user_asm.push_str(&runtime::emit_class_methods_registry_data(module));
        user_asm.push('\n');
        user_asm.push_str(&runtime::emit_member_exists_registry_data(module));
    }
    // The late passes read the spilled prefix through a clean, file-backed mapping.
    let prefix = user_assembly::PrefixView::open(spilled.as_ref())
        .map_err(|error| CodegenIrError::io("cannot read the spilled user assembly", error))?;
    let edits =
        aarch64_relax::conditional_branch_relaxations(&[prefix.as_str(), &text], module.target);
    let mut exported: HashSet<String> = exported_functions
        .values()
        .map(|export| module.target.extern_symbol(&export.c_name))
        .collect();
    match emit {
        Emit::Cdylib | Emit::Staticlib | Emit::Phpext => {
            for lifecycle in [
                "elephc_abi_version",
                "elephc_init",
                "elephc_shutdown",
                "elephc_last_status",
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
    // The GCC driver contributes the ELF CRT `_init`/`_fini` definitions after
    // assembly. Hidden undefined declarations here propagate local visibility
    // to those definitions in the final shared object.
    let additional_internal: &[&str] = if matches!(emit, Emit::Cdylib)
        && module.target.platform == platform::Platform::Linux
    {
        &["_init", "_fini"]
    } else {
        &[]
    };
    let generated_runtime_features = shared.generated_runtime_features();
    let footer = crate::codegen::visibility::hidden_directives_suffix(
        &[prefix.as_str(), &text, &user_asm],
        &exported,
        module.target.platform,
        additional_internal,
    );
    drop(prefix);
    let mut suffix = user_asm;
    suffix.push_str(&footer);
    Ok(GeneratedUserAsm {
        asm: UserAssembly::new(spilled, text, edits, suffix),
        handler_state: generated_runtime_features.handler_state,
        object_clone: generated_runtime_features.object_clone,
    })
}

#[cfg(test)]
mod instrumentation_tests {
    use super::Instrumentation;

    /// Selection decides who pays the per-call cost, so a pattern matching too much
    /// silently reinstates the overhead the flag exists to avoid — and one matching
    /// too little leaves a hole in the profile with nothing to show for it.
    #[test]
    fn selection_matches_exactly_or_by_prefix() {
        let only = Instrumentation::Only(vec![
            "process_order".to_string(),
            "PDOStatement::*".to_string(),
        ]);
        assert!(only.covers("process_order"));
        assert!(only.covers("PDOStatement::execute"));
        assert!(only.covers("PDOStatement::"), "the bare prefix still matches");
        // A name that merely CONTAINS a pattern is not a match: substring matching
        // would sweep in unrelated functions and quietly restore the full cost.
        assert!(!only.covers("run_process_order"));
        assert!(!only.covers("PDO::execute"));
        assert!(!only.covers("format_money"));

        let main_only = Instrumentation::Only(vec!["{main}".to_string()]);
        assert!(main_only.covers("{main}"));
        assert!(!main_only.covers("main"), "the display-root spelling is explicit");

        assert!(Instrumentation::All.covers("anything"));
        assert!(!Instrumentation::Off.covers("anything"));
    }

    /// Only a subset changes what "self" means, so only a subset carries the caveat.
    #[test]
    fn partiality_is_what_triggers_the_caveat() {
        assert!(!Instrumentation::Off.is_partial());
        assert!(
            !Instrumentation::All.is_partial(),
            "full coverage needs no caveat"
        );
        assert!(Instrumentation::Only(vec!["a".to_string()]).is_partial());

        assert!(!Instrumentation::Off.is_on());
        assert!(Instrumentation::All.is_on());
        assert!(Instrumentation::Only(vec!["a".to_string()]).is_on());
    }
}

/// Builds the deterministic scalar subset used by runtime constant lookup.
fn scalar_const_registry(module: &Module) -> Vec<(String, crate::ir::ConstScalar)> {
    let mut registry = module
        .global_constants
        .iter()
        .filter_map(|(name, (value, _))| {
            let scalar = match value {
                ExprKind::IntLiteral(value) => crate::ir::ConstScalar::Int(*value),
                ExprKind::FloatLiteral(value) => crate::ir::ConstScalar::Float(*value),
                ExprKind::BoolLiteral(value) => crate::ir::ConstScalar::Bool(*value),
                ExprKind::StringLiteral(value) => crate::ir::ConstScalar::Str(value.clone()),
                ExprKind::Null => crate::ir::ConstScalar::Null,
                _ => return None,
            };
            Some((name.trim_start_matches('\\').to_string(), scalar))
        })
        .collect::<Vec<_>>();
    registry.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
    registry
}

/// Normalizes registry names to the lowercased sorted runtime-search representation.
fn sorted_lowercased_registry_names<'a>(names: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut normalized = names
        .map(|name| php_symbol_key(name.trim_start_matches('\\')))
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    normalized
}

/// Returns true for compiler-internal classes hidden from PHP class lookup.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}
