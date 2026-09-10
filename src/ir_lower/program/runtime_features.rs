//! Purpose:
//! Runtime-feature discovery from lowered EIR and eval scope state.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;
use crate::ir::{Op, ResourceCleanupKind};

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
    module.required_runtime_features.dom_bridge |= features.dom_bridge;
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
    // Mixed dispatch ladders are assembled after this pass.  Their native-wrapper arms do not
    // carry an `InternalExtensionCall` in EIR, so waiting for that opcode leaves direct
    // `Mixed`/union property and string contexts with unresolved `elephc_dom_*` references.
    features.dom_bridge = module_uses_dynamic_dom_dispatch(module);
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
                // Direct native operations are the ordinary bridge path; dynamic Mixed/union
                // operations are seeded before this loop by `module_uses_dynamic_dom_dispatch`.
                Op::InternalExtensionCall => {
                    features.dom_bridge = true;
                }
                _ => {}
            }
        }
    }
    features
}

/// Returns whether codegen will assemble a native-wrapper arm for a dynamic object operation.
///
/// The checker installs the complete internal-extension declaration surface in `class_infos`.
/// That metadata alone must not select the bridge: only a lowered Mixed/union operation that can
/// match a wrapper method/property (or the SimpleXML dynamic object handlers) does so.
fn module_uses_dynamic_dom_dispatch(module: &Module) -> bool {
    all_lowered_functions(module).any(|function| {
        function.instructions.iter().any(|inst| {
            if !mixed_receiver_operation(function, inst) {
                return false;
            }
            match inst.op {
                // The shared `_eir_shared_mixed_echo`/`_eir_shared_mixed_to_string` helpers are
                // materialized by codegen from these sites. Their synthetic helper function is
                // not present in EIR, and its ladder may contain native-wrapper arms even when
                // this particular site has no statically named DOM method. Treat the site as
                // bridge-capable up front so link planning sees the same requirement as helper
                // emission.
                Op::Cast => matches!(
                    inst.immediate,
                    Some(Immediate::CastTarget(IrType::Str))
                ),
                Op::EchoValue => true,
                Op::MethodCall | Op::NullsafeMethodCall => {
                    let Some(method) = instruction_data_string(module, inst) else {
                        return false;
                    };
                    native_wrapper_method_matches(module, method, inst.operands.len())
                }
                Op::PropGet | Op::NullsafePropGet => {
                    let Some(property) = instruction_data_string(module, inst) else {
                        return false;
                    };
                    native_wrapper_virtual_property_matches(module, property)
                        || (inst.operands.len() >= 4 && simplexml_wrapper_exists(module))
                }
                // A runtime property name can match any declared native virtual property.
                // Four-operand array reads are the equivalent SimpleXML object-handler path.
                Op::DynamicPropGet => native_wrapper_virtual_property_exists(module),
                Op::RuntimeCall if inst.operands.len() >= 4 => simplexml_wrapper_exists(module),
                _ => false,
            }
        })
    })
}

/// Returns whether the first operand of an instruction is a boxed Mixed/union receiver.
fn mixed_receiver_operation(function: &Function, inst: &crate::ir::Instruction) -> bool {
    inst.operands
        .first()
        .and_then(|value| function.value(*value))
        .is_some_and(|value| {
            matches!(value.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        })
}

/// Resolves a data-backed method/property name without turning malformed EIR into a panic.
fn instruction_data_string<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<&'a str> {
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return None,
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns whether one native-wrapper method can match the dynamic call's ABI arity.
fn native_wrapper_method_matches(module: &Module, method: &str, operand_count: usize) -> bool {
    let key = crate::names::php_symbol_key(method);
    module.class_infos.iter().any(|(class_name, class_info)| {
        if !is_native_wrapper_or_descendant(module, class_name) {
            return false;
        }
        let Some(signature) = class_info.methods.get(&key) else {
            return false;
        };
        let supplied = operand_count.saturating_sub(1);
        supplied == signature.params.len()
            || signature
                .variadic
                .as_ref()
                .is_some_and(|_| {
                    supplied >= crate::types::call_args::regular_param_count(signature)
                })
    })
}

/// Returns whether a named property is implemented by a native virtual handler.
fn native_wrapper_virtual_property_matches(module: &Module, property: &str) -> bool {
    module.class_infos.iter().any(|(class_name, class_info)| {
        if !is_native_wrapper_or_descendant(module, class_name)
            || !class_info.properties.iter().any(|(name, _)| name == property)
        {
            return false;
        }
        let declaring = class_info
            .property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        crate::internal_extensions::operation_registry()
            .property(declaring, property, false)
            .is_some()
    })
}

/// Returns whether any native wrapper exposes a virtual property or SimpleXML handler.
fn native_wrapper_virtual_property_exists(module: &Module) -> bool {
    module.class_infos.iter().any(|(class_name, class_info)| {
        is_native_wrapper_or_descendant(module, class_name)
            && class_info.properties.iter().any(|(property, _)| {
                let declaring = class_info
                    .property_declaring_classes
                    .get(property)
                    .map(String::as_str)
                    .unwrap_or(class_name.as_str());
                crate::internal_extensions::operation_registry()
                    .property(declaring, property, false)
                    .is_some()
            })
    })
}

/// Returns true when the locked surface contains a native wrapper in the SimpleXML family.
fn simplexml_wrapper_exists(module: &Module) -> bool {
    module.class_infos.keys().any(|class_name| {
        is_native_wrapper_or_descendant(module, class_name)
            && is_simplexml_class(module, class_name)
    })
}

/// Returns whether `class_name` is a native wrapper or inherits from one.
fn is_native_wrapper_or_descendant(module: &Module, class_name: &str) -> bool {
    crate::internal_extensions::is_native_wrapper_class(class_name)
        || crate::internal_extensions::is_native_wrapper_descendant(
            &module.class_infos,
            class_name,
        )
}

/// Walks the checked parent chain to identify a SimpleXML wrapper or descendant.
fn is_simplexml_class(module: &Module, class_name: &str) -> bool {
    let mut current = Some(class_name.to_string());
    for _ in 0..=module.class_infos.len() {
        let Some(name) = current else {
            break;
        };
        if name.eq_ignore_ascii_case("SimpleXMLElement") {
            return true;
        }
        current = module
            .class_infos
            .get(&name)
            .and_then(|class_info| class_info.parent.clone());
    }
    false
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
