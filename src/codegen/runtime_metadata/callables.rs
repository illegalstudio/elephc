//! Purpose:
//! Builds runtime-visible function signatures and intrinsic method wrappers.
//!
//! Called from:
//! - `super::super::finalize_user_asm()` and class metadata filtering.
//!
//! Key details:
//! - Excludes synthetic property initializers and preserves include-variant callable groups.

use super::*;

/// Returns user functions visible to runtime callable-name metadata.
pub(in crate::codegen) fn runtime_user_function_sigs(module: &Module) -> HashMap<String, FunctionSig> {
    let mut functions = module
        .functions
        .iter()
        .filter(|function| !is_property_init_thunk_function(function))
        .map(|function| (function.name.clone(), ir_function_sig(function)))
        .collect::<HashMap<_, _>>();
    for group in function_variants::collect_dispatch_groups(module) {
        if let Some(function) = function_variants::variant_callee_for_group(module, &group.name) {
            functions
                .entry(group.name.clone())
                .or_insert_with(|| ir_function_sig(function));
        }
    }
    functions
}

/// Returns true for synthetic property-default init thunks, which are not PHP callables.
pub(in crate::codegen) fn is_property_init_thunk_function(function: &Function) -> bool {
    function.name.starts_with("_class_propinit_")
}

/// Reconstructs callable metadata from an EIR function when no source signature is attached.
pub(in crate::codegen) fn ir_function_sig(function: &Function) -> FunctionSig {
    if let Some(signature) = &function.signature {
        return signature.clone();
    }
    FunctionSig {
        params: function
            .params
            .iter()
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        param_type_exprs: vec![None; function.params.len()],
        param_attributes: vec![Vec::new(); function.params.len()],
        defaults: vec![None; function.params.len()],
        return_type: function.return_php_type.clone(),
        declared_return: false,
        by_ref_return: function.flags.by_ref_return,
        ref_params: function.params.iter().map(|param| param.by_ref).collect(),
        declared_params: vec![true; function.params.len()],
        variadic: function
            .params
            .iter()
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
        is_generator: false,
    }
}

/// Returns include-variant public names that runtime callable lookup must check dynamically.
pub(in crate::codegen) fn runtime_function_variant_groups(module: &Module) -> HashSet<String> {
    function_variants::collect_dispatch_groups(module)
        .into_iter()
        .map(|group| group.name)
        .collect()
}

/// Returns true when runtime callable helpers need broad user callable metadata.
pub(in crate::codegen) fn module_uses_dynamic_callable_lookup(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .any(function_uses_dynamic_callable_lookup)
}

/// Returns true when one function calls `is_callable()` on a runtime-shaped value.
pub(in crate::codegen) fn function_uses_dynamic_callable_lookup(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        if !is_dynamic_callable_lookup_builtin(inst) || inst.operands.is_empty() {
            return false;
        }
        let Some(value) = function.value(inst.operands[0]) else {
            return false;
        };
        matches!(
            value.php_type.codegen_repr(),
            PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Iterable
        )
    })
}

/// Returns true for an EIR builtin instruction that calls PHP `is_callable()`.
pub(in crate::codegen) fn is_dynamic_callable_lookup_builtin(inst: &crate::ir::Instruction) -> bool {
    typed_builtin_target(inst).is_some_and(|target| target.is_callable_lookup())
}

/// Emits method-symbol wrappers for runtime-backed intrinsic class methods.
pub(in crate::codegen) fn emit_intrinsic_method_wrappers(module: &Module, emitter: &mut Emitter) {
    for wrapper in intrinsic_method_wrapper_specs(module) {
        emit_intrinsic_method_wrapper(emitter, &wrapper);
    }
}

/// Forwards one native method ABI and publishes its result ownership after the helper returns.
fn emit_intrinsic_method_wrapper(emitter: &mut Emitter, wrapper: &IntrinsicMethodWrapper) {
    let symbol = if wrapper.is_static {
        static_method_symbol(&wrapper.class_name, &wrapper.method_key)
    } else {
        method_symbol(&wrapper.class_name, &wrapper.method_key)
    };
    emitter.label(&symbol);
    let frame_size = (wrapper.arg_types.len() + 1) * 16;
    abi::emit_frame_prologue(emitter, frame_size);
    let mut cursor = abi::IncomingArgCursor::for_target(emitter.target, 0);
    for (index, ty) in wrapper.arg_types.iter().enumerate() {
        abi::emit_store_incoming_param(emitter, "native_method_arg", ty,
            (index + 1) * 16, false, &mut cursor);
    }
    // A method ABI lends its parameters; consuming native SPL helpers need
    // their own value cells, just like a compiled PHP parameter shadow.
    for parameter in &wrapper.consumed_mixed_parameters {
        let offset = (parameter + 2) * 16;
        abi::emit_load(emitter, &PhpType::Mixed, offset);
        abi::emit_call_label(emitter, "__rt_mixed_clone");
        abi::store_at_offset(emitter, abi::int_result_reg(emitter), offset);
        let owner = abi::tertiary_scratch_reg(emitter);
        abi::emit_frame_slot_address(emitter, owner, offset);
        abi::emit_push_call_operand_owner(emitter, owner, false);
    }
    for _ in &wrapper.consumed_mixed_parameters { abi::emit_pop_call_operand_owner(emitter); }
    // Forward through the ordinary argument planner, including overflow
    // words. The wrapper frame must not shift a native helper's arguments.
    for (index, ty) in wrapper.arg_types.iter().enumerate() {
        abi::emit_load(emitter, ty, (index + 1) * 16);
        abi::emit_push_result_value(emitter, ty);
    }
    let assignments = abi::build_outgoing_arg_assignments_for_target(
        emitter.target, &wrapper.arg_types, 0,
    );
    let overflow = abi::materialize_outgoing_args(emitter, &assignments);
    let pad = abi::outgoing_call_stack_pad_bytes(emitter.target, overflow);
    abi::emit_reserve_temporary_stack(emitter, pad);
    abi::emit_call_label(emitter, wrapper.helper);
    abi::emit_release_temporary_stack(emitter, pad);
    abi::emit_release_temporary_stack(emitter, overflow);
    // Native helpers do not implement the private PHP return protocol.
    // In particular, legacy SPL serialization lends concat scratch.
    abi::emit_php_return_status(emitter, wrapper.returns_owned);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Runtime-backed method wrapper that should be emitted as a PHP method symbol.
pub(in crate::codegen) struct IntrinsicMethodWrapper {
    pub(in crate::codegen) class_name: String,
    pub(in crate::codegen) method_key: String,
    helper: &'static str,
    pub(in crate::codegen) is_static: bool,
    arg_types: Vec<PhpType>,
    returns_owned: bool,
    consumed_mixed_parameters: Vec<usize>,
}

/// Describes the physical words forwarded by an intrinsic wrapper without coercing them.
fn intrinsic_wrapper_arg_types(class_name: &str, signature: &FunctionSig, is_static: bool) -> Vec<PhpType> {
    let mut types = vec![if is_static { PhpType::Int } else { PhpType::Object(class_name.into()) }];
    types.extend(signature.params.iter().enumerate().map(|(index, (_, ty))| {
        if signature.ref_params.get(index).copied().unwrap_or(false) { PhpType::Int }
        else { ty.codegen_repr() }
    }));
    types
}

/// Returns intrinsic instance/static methods that need method-symbol wrappers.
pub(in crate::codegen) fn intrinsic_method_wrapper_specs(module: &Module) -> Vec<IntrinsicMethodWrapper> {
    let eir_methods = eir_class_method_keys(module);
    let mut wrappers = Vec::new();
    for (class_name, class_info) in &module.class_infos {
        for (method_key, signature) in &class_info.methods {
            let impl_class = class_info
                .method_impl_classes
                .get(method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if eir_methods.contains(&(impl_class.to_string(), method_key.clone(), false)) {
                continue;
            }
            if let Some((intrinsic, helper)) = IntrinsicCall::instance_method(impl_class, method_key)
                .and_then(|intrinsic| intrinsic.runtime_helper().map(|helper| (intrinsic, helper)))
            {
                wrappers.push(IntrinsicMethodWrapper {
                    class_name: impl_class.to_string(),
                    method_key: method_key.clone(),
                    helper,
                    is_static: false,
                    arg_types: intrinsic_wrapper_arg_types(impl_class, signature, false),
                    returns_owned: signature.return_type.codegen_repr().is_refcounted()
                        || signature.return_type.codegen_repr() == PhpType::Callable,
                    consumed_mixed_parameters: intrinsic.consumed_mixed_parameters().to_vec(),
                });
            }
        }
        for (method_key, signature) in &class_info.static_methods {
            let impl_class = class_info
                .static_method_impl_classes
                .get(method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if eir_methods.contains(&(impl_class.to_string(), method_key.clone(), true)) {
                continue;
            }
            if let Some((intrinsic, helper)) = IntrinsicCall::static_method(impl_class, method_key)
                .and_then(|intrinsic| intrinsic.runtime_helper().map(|helper| (intrinsic, helper)))
            {
                wrappers.push(IntrinsicMethodWrapper {
                    class_name: impl_class.to_string(),
                    method_key: method_key.clone(),
                    helper,
                    is_static: true,
                    arg_types: intrinsic_wrapper_arg_types(impl_class, signature, true),
                    returns_owned: signature.return_type.codegen_repr().is_refcounted()
                        || signature.return_type.codegen_repr() == PhpType::Callable,
                    consumed_mixed_parameters: intrinsic.consumed_mixed_parameters().to_vec(),
                });
            }
        }
    }
    wrappers.sort_by(|left, right| {
        (&left.class_name, &left.method_key, left.is_static).cmp(&(
            &right.class_name,
            &right.method_key,
            right.is_static,
        ))
    });
    wrappers.dedup_by(|left, right| {
        left.class_name == right.class_name
            && left.method_key == right.method_key
            && left.is_static == right.is_static
    });
    wrappers
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native wrappers preserve incoming overflow words and publish status after the native call.
    #[test]
    fn intrinsic_wrapper_forwards_stack_arguments_and_publishes_return_owner_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for owned in [false, true] {
                let mut args = vec![PhpType::Object("NativeOwnerProbe".into())];
                args.extend(std::iter::repeat_n(PhpType::Int, 8));
                let wrapper = IntrinsicMethodWrapper {
                    class_name: "NativeOwnerProbe".into(), method_key: "probe".into(),
                    helper: "_native_owner_probe", is_static: false,
                    arg_types: args, returns_owned: owned, consumed_mixed_parameters: Vec::new(),
                };
                let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(target).unwrap());
                emit_intrinsic_method_wrapper(&mut emitter, &wrapper);
                let asm = emitter.output();
                let call = if target == "linux-x86_64" { "call _native_owner_probe" } else { "bl _native_owner_probe" };
                let status = match (target == "linux-x86_64", owned) {
                    (true, true) => "mov r11d, 1", (true, false) => "xor r11d, r11d",
                    (false, true) => "mov x15, #1", (false, false) => "mov x15, xzr",
                };
                assert!(asm.find(call).unwrap() < asm.find(status).unwrap(), "{target}: status must survive helper clobbers");
                assert!(asm.contains(if target == "linux-x86_64" { "[rbp + 16]" } else { "[x29, #16]" }),
                    "{target}: incoming stack arguments must be reloaded before forwarding");
            }
        }
    }
}
