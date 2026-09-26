//! Purpose:
//! Runtime-feature discovery from lowered EIR and eval scope state.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;
use crate::ir::{CoreBuiltinOp, ResourceCleanupKind, RuntimeFnId};

/// Adds optional runtime features referenced by synthetic or lowered EIR functions.
pub(in crate::ir_lower) fn include_lowered_runtime_features(module: &mut Module) {
    let features = lowered_runtime_features(module);
    module.required_runtime_features.regex |= features.regex;
    module.required_runtime_features.mb_strlen |= features.mb_strlen;
    module.required_runtime_features.phar_archive |= features.phar_archive;
    module.required_runtime_features.descriptor_invoker |= features.descriptor_invoker;
    module.required_runtime_features.pdo_udf |= features.pdo_udf;
    module.required_runtime_features.eval_bridge |= features.eval_bridge;
    module.required_runtime_features.eval_scope |= features.eval_scope;
    module.required_runtime_features.popen_resource |= features.popen_resource;
    module.required_runtime_features.directory_resource |= features.directory_resource;
    module.required_runtime_features.handler_state |= features.handler_state;
    module.required_runtime_features.object_clone |= features.object_clone;
    // Not derived from the instruction stream like the rest: a Fiber object can only exist if the
    // builtin class was registered, and `types::checker::builtin_class_gate` has already decided
    // that from the program's own text. Reading the answer here is exact, where scanning EIR for
    // "something that makes a fiber" would be an approximation of it.
    module.required_runtime_features.fiber |= module.class_infos.contains_key("Fiber");
    module.required_runtime_features.generator |= module.class_infos.contains_key("Generator");
}

/// Returns whether this call is `__elephc_opcache_rt_compile`.
///
/// Matched on the runtime-function id rather than the builtin target, because these helpers
/// are internal registry entries reached through `RuntimeCallTarget::Function`.
fn runtime_call_targets_opcache_compile(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(crate::ir::Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ElephcOpcacheRtCompile)
        ))
    )
}

/// Returns whether this call asks the ON-DISK cache: the file-cache query, or either half of
/// `opcache_invalidate()`, which removes the disk entry as well as the memory one.
fn runtime_call_reaches_the_file_cache(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ElephcOpcacheRtInFileCache
                | crate::ir::RuntimeFnId::ElephcOpcacheRtDiscard
                | crate::ir::RuntimeFnId::ElephcOpcacheRtSoftInvalidate
        )))
    )
}

/// Returns whether this compilation has an `opcache.file_cache` that is actually live: OPcache
/// enabled and a directory configured. Read from the same compile settings the startup
/// configuration is built from, which the pipeline installs before lowering runs.
fn compile_configures_a_live_file_cache() -> bool {
    let config = crate::opcache::runtime_cache::runtime_cache_config(
        crate::codegen_support::compile_php_version().version_id(),
        crate::codegen_support::compile_is_web_sapi(),
        &crate::codegen_support::ini_overrides(),
    );
    config.enabled && !config.file_cache.is_empty()
}

/// Derives optional runtime features from the actual EIR instruction stream.
pub(super) fn lowered_runtime_features(module: &Module) -> RuntimeFeatures {
    lowered_runtime_features_with(module, true)
}

/// Whether this program can EXECUTE interpreted code: `eval()`, a dynamic include, anything
/// that runs through the interpreter — as opposed to linking it only for an OPcache operation.
///
/// `opcache_compile_file()` and the file-cache operations link the eval bridge (see below) but
/// execute nothing through it: `opcache_compile_file()` parses and caches without running.
/// The post-link note that "evaluated code that uses preg_* will fail" was printed for them
/// too, about code such a program cannot run. Reported by DeepSeek.
///
/// Called only by the compiler binary's pipeline, which the lib target does not include, hence
/// the dead-code allowance there.
#[allow(dead_code)]
pub(crate) fn module_runs_interpreted_code(module: &Module) -> bool {
    lowered_runtime_features_with(module, false).eval_bridge
}

/// [`lowered_runtime_features`], optionally leaving out the OPcache operations' bridge links.
fn lowered_runtime_features_with(module: &Module, count_opcache_links: bool) -> RuntimeFeatures {
    let mut features = RuntimeFeatures::none();
    // Computed lazily and once: only a program that names a file-cache operation pays for it.
    let mut live_file_cache: Option<bool> = None;
    for function in all_lowered_functions(module) {
        if function_contains_eval_scope_state(function) {
            features.eval_scope = true;
        }
        if function_contains_eval_context_state(function) {
            features.eval_bridge = true;
        }
        for (inst_index, inst) in function.instructions.iter().enumerate() {
            match inst.op {
                Op::RuntimeCall => {
                    // `opcache_compile_file()` is the ONE runtime-tier operation whose job is
                    // to CREATE a cache entry, so it is the one whose pay-for-use fold is a
                    // lie rather than a truth. `is_cached` and `discard` fold to `false` in a
                    // binary with no dynamic tier and that IS the right answer — nothing can
                    // have been cached. `compile_file` folding to `false` instead reports a
                    // refusal reference PHP never issues: it returns `true` and the file is in
                    // the cache, whether or not the program uses `eval()`.
                    //
                    // So calling it is itself a reason to link the bridge. The cost lands
                    // exactly on programs that ask for the dynamic tier, which is what
                    // pay-for-use means — and no wider than that: with OPcache disabled at
                    // compile time the prelude's own gate short-circuits before this call is
                    // ever emitted, so a disabled build never reaches here and stays small.
                    if count_opcache_links && runtime_call_targets_opcache_compile(inst) {
                        features.eval_bridge = true;
                    }
                    // THE SAME ARGUMENT, FOR THE DISK. `is_cached` and `discard` may fold
                    // because nothing can have been cached in a binary with no dynamic tier —
                    // but a configured `opcache.file_cache` is shared with OTHER processes, so
                    // an entry can be on disk whatever this binary did. Folding the file-cache
                    // query to `false` then reported a miss reference never reports, and
                    // folding `opcache_invalidate()` left another process's entry on disk.
                    // MEASURED: a reader with no `eval()` asked about an entry a seed process
                    // left; reference answered `true`, elephc `false`.
                    //
                    // Gated on the compile-time configuration, so a build with no file cache —
                    // php-src's default — still folds and stays small.
                    if count_opcache_links
                        && runtime_call_reaches_the_file_cache(inst)
                        && *live_file_cache.get_or_insert_with(compile_configures_a_live_file_cache)
                    {
                        features.eval_bridge = true;
                    }
                    if let Some(target) = typed_builtin_target(inst) {
                        features.regex |= target.uses_regex_runtime();
                        features.mb_strlen |= target.uses_mb_strlen_runtime();
                        features.phar_archive |= target.publishes_phar_symbols()
                            && function_belongs_to_phar_archive_helper_class(function);
                        features.descriptor_invoker |=
                            typed_builtin_requires_descriptor_invoker(function, inst, target);
                        // The resource a builtin boxes is the ONLY way its cleanup kind reaches
                        // `__rt_mixed_free_deep`: eval boxes its own handles as kind 0, and a
                        // callable named by a runtime-unknown string is a fatal, not a dispatch.
                        // So a call in this stream is exactly the condition for the arm.
                        match target.resource_cleanup_kind() {
                            Some(ResourceCleanupKind::PopenPipe) => features.popen_resource = true,
                            Some(ResourceCleanupKind::Directory) => {
                                features.directory_resource = true
                            }
                            Some(ResourceCleanupKind::StreamFd) | None => {}
                        }
                        features.object_clone |= target == RuntimeFnId::CloneWith;
                    }
                }
                Op::CoreBuiltin => {
                    let operation = match inst.immediate {
                        Some(Immediate::I64(value)) => CoreBuiltinOp::from_i64(value),
                        _ => None,
                    };
                    features.handler_state |= matches!(
                        operation,
                        Some(
                            CoreBuiltinOp::RestoreErrorHandler
                                | CoreBuiltinOp::RestoreExceptionHandler
                                | CoreBuiltinOp::SetErrorHandler
                                | CoreBuiltinOp::SetExceptionHandler
                                | CoreBuiltinOp::GetErrorHandler
                                | CoreBuiltinOp::GetExceptionHandler
                        )
                    );
                }
                Op::LanguageConstructCall => {
                    if language_construct_call_requires_eval(module, inst) {
                        features.eval_bridge = true;
                    }
                }
                Op::EvalLiteralCall => {
                    if eval_literal_call_requires_bridge(module, function, inst_index, inst) {
                        features.eval_bridge = true;
                    }
                }
                Op::EvalScopeGet | Op::EvalScopeSet => {
                    features.eval_scope = true;
                }
                Op::EvalFunctionCall
                | Op::EvalFunctionCallArray
                | Op::EvalFunctionExists
                | Op::EvalClassExists
                | Op::EvalConstantExists
                | Op::EvalConstantFetch
                | Op::EvalStaticMethodCall => {
                    features.eval_bridge = true;
                }
                Op::ExprCall | Op::CallableDescriptorInvoke => {
                    features.descriptor_invoker = true;
                }
                Op::PdoAdapterAddr => {
                    features.pdo_udf = true;
                }
                _ => {}
            }
        }
    }
    features
}

/// Returns true when a lowered function owns hidden eval scope handle slots.
/// Scope-only functions use the native scope helpers and must not force the
/// magician bridge staticlib into the link.
pub(super) fn function_contains_eval_scope_state(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Returns true when a lowered function owns an interpreter context handle,
/// which requires the full magician eval bridge runtime.
pub(super) fn function_contains_eval_context_state(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| matches!(local.kind, LocalKind::EvalContext))
}

/// Returns true when a literal eval call still needs the magician bridge runtime.
pub(crate) fn eval_literal_call_requires_bridge(
    module: &Module,
    function: &Function,
    inst_index: usize,
    inst: &crate::ir::Instruction,
) -> bool {
    let (data, strict_php) = match inst.immediate {
        Some(Immediate::Data(data)) => (data, false),
        Some(Immediate::ProfiledData { data, strict_php }) => (data, strict_php),
        _ => return true,
    };
    let Some(fragment) = module.data.strings.get(data.as_raw() as usize) else {
        return true;
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        module.source_path.as_deref(),
        strict_php,
        |name, args| eval_literal_static_function_supported_by_module(module, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_module(module, receiver, method, args)
        },
    );
    if plan.uses_scope_read_params() {
        return !eval_literal_call_can_use_scope_read_params(module, function, inst_index, &plan);
    }
    if plan.requires_runtime_eval_scope()
        && !eval_literal_call_scope_constraints_supported(module, function, inst_index, &plan)
    {
        return true;
    }
    plan.requires_runtime_eval_bridge()
}

/// Returns true when a local slot is initialized before the eval instruction.
pub(super) fn eval_scope_read_slot_initialized(
    function: &Function,
    slot: crate::ir::LocalSlotId,
    inst_index: usize,
) -> bool {
    if function
        .params
        .get(slot.as_raw() as usize)
        .is_some_and(|param| !param.by_ref)
    {
        return true;
    }
    function
        .instructions
        .iter()
        .take(inst_index)
        .any(|inst| inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot)))
}

/// Returns true when a read-only eval call can pass direct Mixed params safely.
pub(super) fn eval_literal_call_can_use_scope_read_params(
    module: &Module,
    function: &Function,
    inst_index: usize,
    plan: &crate::eval_aot::EvalAotPlan,
) -> bool {
    plan.reads().iter().all(|name| {
        eval_literal_call_scope_read_param_supported(
            module,
            function,
            inst_index,
            name,
            plan.quiet_reads().contains(name),
        )
    }) && plan.array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_array_param_supported(module, function, inst_index, name)
    }) && plan.assoc_array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_assoc_array_param_supported(module, function, inst_index, name)
    }) && plan.float_predicate_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_float_predicate_param_supported(
            module, function, inst_index, name,
        )
    })
}

/// Returns true when scope-based eval AOT satisfies caller-side type constraints.
pub(super) fn eval_literal_call_scope_constraints_supported(
    module: &Module,
    function: &Function,
    inst_index: usize,
    plan: &crate::eval_aot::EvalAotPlan,
) -> bool {
    plan.array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_array_param_supported(module, function, inst_index, name)
    }) && plan.assoc_array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_assoc_array_param_supported(module, function, inst_index, name)
    }) && plan.float_predicate_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_float_predicate_param_supported(
            module, function, inst_index, name,
        )
    })
}

/// Returns true when one caller read is initialized and can be boxed directly.
pub(super) fn eval_literal_call_scope_read_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
    quiet: bool,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return quiet;
    };
    eval_scope_read_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read is initialized with an array-compatible type.
pub(super) fn eval_literal_call_scope_read_array_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_array_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read is initialized with an associative-array type.
pub(super) fn eval_literal_call_scope_read_assoc_array_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_assoc_array_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read can feed IEEE float predicates safely.
pub(super) fn eval_literal_call_scope_read_float_predicate_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_float_predicate_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when a caller local can be boxed into a direct eval read param.
pub(super) fn eval_scope_read_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a caller local satisfies array-only read-param semantics.
pub(super) fn eval_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a caller local satisfies associative-array-only semantics.
pub(super) fn eval_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a caller local can feed IEEE float predicates without TypeError.
pub(super) fn eval_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Returns the caller local slot that can provide a direct scope-read parameter.
pub(super) fn eval_scope_local_slot<'a>(
    function: &'a Function,
    name: &str,
) -> Option<&'a crate::ir::LocalSlot> {
    function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some(name) && local.kind == LocalKind::PhpLocal)
}

/// Returns true when a static function call matches the codegen-supported subset.
pub(super) fn eval_literal_static_function_supported_by_module(
    module: &Module,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(function) = module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
    else {
        return false;
    };
    let Some(signature) = &function.signature else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a static method call matches the codegen-supported subset.
pub(super) fn eval_literal_static_method_supported_by_module(
    module: &Module,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let class_name = class_name.as_str().trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let Some(receiver_info) = module.class_infos.get(class_name) else {
        return false;
    };
    if receiver_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let Some(signature) = module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&method_key))
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}
