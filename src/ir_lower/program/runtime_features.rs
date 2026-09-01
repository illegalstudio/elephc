//! Purpose:
//! Runtime-feature discovery from lowered EIR and eval scope state.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;
use crate::ir::ResourceCleanupKind;

/// Adds optional runtime features referenced by synthetic or lowered EIR functions.
pub(in crate::ir_lower) fn include_lowered_runtime_features(module: &mut Module) {
    let features = lowered_runtime_features(module);
    module.required_runtime_features.regex |= features.regex;
    module.required_runtime_features.timelib |= features.timelib;
    module.required_runtime_features.mb_strlen |= features.mb_strlen;
    module.required_runtime_features.phar_archive |= features.phar_archive;
    module.required_runtime_features.descriptor_invoker |= features.descriptor_invoker;
    module.required_runtime_features.pdo_udf |= features.pdo_udf;
    module.required_runtime_features.eval_bridge |= features.eval_bridge;
    module.required_runtime_features.eval_scope |= features.eval_scope;
    module.required_runtime_features.popen_resource |= features.popen_resource;
    module.required_runtime_features.directory_resource |= features.directory_resource;
    // Not derived from the instruction stream like the rest: a Fiber object can only exist if the
    // builtin class was registered, and `types::checker::builtin_class_gate` has already decided
    // that from the program's own text. Reading the answer here is exact, where scanning EIR for
    // "something that makes a fiber" would be an approximation of it.
    module.required_runtime_features.fiber |= module.class_infos.contains_key("Fiber");
    module.required_runtime_features.generator |= module.class_infos.contains_key("Generator");
}

/// Derives optional runtime features from the actual EIR instruction stream.
pub(super) fn lowered_runtime_features(module: &Module) -> RuntimeFeatures {
    let mut features = RuntimeFeatures::none();
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
                    if let Some(target) = typed_builtin_target(inst) {
                        features.timelib |= matches!(
                            target,
                            crate::ir::RuntimeFnId::ElephcStrtotimeRaw
                                | crate::ir::RuntimeFnId::Strtotime
                                | crate::ir::RuntimeFnId::Date
                                | crate::ir::RuntimeFnId::Gmdate
                        );
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
                    }
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
        eval_literal_call_scope_read_param_supported(module, function, inst_index, name)
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

/// Returns true when one caller read can be boxed or represented as undefined null.
pub(super) fn eval_literal_call_scope_read_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return true;
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
