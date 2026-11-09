//! Purpose:
//! Collects AOT callable, property, and attribute metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Only signatures representable by the bridge are exposed.

use super::*;

/// Collects global PHP functions that can use the descriptor-invoker bridge.
///
/// A conditionally declared function (`if (!function_exists('f')) { function f() {} }`) is a
/// variant behind a public-name dispatcher; it is registered under that PUBLIC name, calling the
/// dispatcher, and exists for eval only while its active-variant slot is filled. The variants'
/// own internal names are never PHP-visible, so they are not registered.
pub(super) fn eval_native_function_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeFunctionRegistration> {
    let groups = crate::ir::function_variants::collect_dispatch_groups(ctx.module);
    let variant_names: crate::fast_hash::FastSet<String> = groups
        .iter()
        .flat_map(|group| group.variants.iter().map(|name| php_symbol_key(name)))
        .collect();
    let mut registrations: Vec<EvalNativeFunctionRegistration> = ctx
        .module
        .functions
        .iter()
        .filter(|function| function_has_eval_metadata(function))
        .filter(|function| !variant_names.contains(&php_symbol_key(&function.name)))
        .map(|function| EvalNativeFunctionRegistration {
            name: function.name.clone(),
            signature: function_signature_from_eir(function),
            bridge_supported: eval_native_function_bridge_supported(function),
            active_slot_symbol: None,
            is_internal: function.flags.is_internal,
        })
        .collect();
    for group in &groups {
        if is_compiler_internal_function_name(&group.name) {
            continue;
        }
        let Some(variant) =
            crate::ir::function_variants::variant_callee_for_group(ctx.module, &group.name)
        else {
            continue;
        };
        registrations.push(EvalNativeFunctionRegistration {
            name: group.name.clone(),
            signature: function_signature_from_eir(variant),
            // The variant's own name is internal (`__elephc_conditional_fn_…`), which
            // `function_has_eval_metadata` refuses; only its signature decides here.
            bridge_supported: function_signature_can_bridge_with_eval(variant),
            active_slot_symbol: Some(crate::names::function_variant_active_symbol(&group.name)),
            // A conditionally declared function is user code, never a PHP internal.
            is_internal: false,
        });
    }
    registrations
}

/// Collects AOT method signatures whose metadata can be exposed to eval.
pub(super) fn eval_native_method_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeMethodRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_eval_native_instance_methods(class_name, class_info, &mut registrations);
        collect_eval_native_static_methods(class_name, class_info, &mut registrations);
    }
    let mut interfaces = ctx.module.interface_infos.iter().collect::<Vec<_>>();
    interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    for (interface_name, interface_info) in interfaces {
        collect_eval_native_interface_instance_methods(
            interface_name,
            interface_info,
            &mut registrations,
        );
        collect_eval_native_interface_static_methods(
            interface_name,
            interface_info,
            &mut registrations,
        );
    }
    registrations
}

/// Collects AOT constructors whose metadata can be exposed to eval.
pub(super) fn eval_native_constructor_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeConstructorRegistration> {
    let method_key = php_symbol_key("__construct");
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        let Some(signature) = class_info.methods.get(&method_key) else {
            continue;
        };
        let bridge_supported = class_method_visibility_bridge_supported(class_info, &method_key)
            && constructor_signature_can_bridge_with_eval(signature);
        registrations.push(EvalNativeConstructorRegistration {
            class_name: class_name.clone(),
            signature: signature.clone(),
            bridge_supported,
        });
    }
    registrations
}

/// Collects AOT property types whose declared PHP type can be exposed to eval reflection.
pub(super) fn eval_native_property_type_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativePropertyTypeRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_eval_native_instance_property_types(class_name, class_info, &mut registrations);
        collect_eval_native_static_property_types(class_name, class_info, &mut registrations);
    }
    registrations
}

/// Collects AOT interface property contracts that eval can validate at declaration time.
pub(super) fn eval_native_interface_property_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeInterfacePropertyRegistration> {
    let mut registrations = Vec::new();
    let mut interfaces = ctx.module.interface_infos.iter().collect::<Vec<_>>();
    interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    for (interface_name, interface_info) in interfaces {
        let mut property_names = interface_info.property_order.iter().collect::<Vec<_>>();
        if property_names.is_empty() {
            property_names = interface_info.properties.keys().collect();
            property_names.sort();
        }
        for property_name in property_names {
            let Some(contract) = interface_info.properties.get(property_name) else {
                continue;
            };
            let Some(registration) = eval_native_interface_property_registration(
                interface_name,
                property_name,
                contract,
            ) else {
                continue;
            };
            registrations.push(registration);
        }
    }
    registrations
}

/// Collects AOT abstract class property contracts that eval can validate at declaration time.
pub(super) fn eval_native_abstract_property_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeAbstractPropertyRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        let mut property_names = class_info
            .abstract_property_hooks
            .keys()
            .collect::<Vec<_>>();
        property_names.sort();
        for property_name in property_names {
            let Some(contract) = class_info.abstract_property_hooks.get(property_name) else {
                continue;
            };
            let Some(registration) =
                eval_native_abstract_property_registration(class_name, property_name, contract)
            else {
                continue;
            };
            registrations.push(registration);
        }
    }
    registrations
}

/// Converts one static abstract class property contract into eval-native metadata.
pub(super) fn eval_native_abstract_property_registration(
    class_name: &str,
    property_name: &str,
    contract: &PropertyHookContract,
) -> Option<EvalNativeAbstractPropertyRegistration> {
    let requires_get = contract.get_type.is_some();
    let requires_set = contract.set_type.is_some();
    if !requires_get && !requires_set {
        return None;
    }
    let type_spec = eval_native_interface_property_type_spec(contract)?;
    Some(EvalNativeAbstractPropertyRegistration {
        class_name: class_name.to_string(),
        declaring_class_name: contract.declaring_type.clone(),
        property_name: property_name.to_string(),
        type_spec,
        requires_get,
        requires_set,
        get_by_ref: contract.get_by_ref,
    })
}

/// Converts one static interface property contract into eval-native metadata.
pub(super) fn eval_native_interface_property_registration(
    interface_name: &str,
    property_name: &str,
    contract: &PropertyHookContract,
) -> Option<EvalNativeInterfacePropertyRegistration> {
    let requires_get = contract.get_type.is_some();
    let requires_set = contract.set_type.is_some();
    if !requires_get && !requires_set {
        return None;
    }
    let type_spec = eval_native_interface_property_type_spec(contract)?;
    Some(EvalNativeInterfacePropertyRegistration {
        interface_name: interface_name.to_string(),
        declaring_interface_name: contract.declaring_type.clone(),
        property_name: property_name.to_string(),
        type_spec,
        requires_get,
        requires_set,
        get_by_ref: contract.get_by_ref,
    })
}

/// Returns the single property type representation accepted by EvalIR metadata.
pub(super) fn eval_native_interface_property_type_spec(contract: &PropertyHookContract) -> Option<String> {
    match (contract.get_type.as_ref(), contract.set_type.as_ref()) {
        (Some(get_type), Some(set_type)) if get_type == set_type => {
            eval_native_php_type_spec(get_type, false)
        }
        (Some(get_type), None) => eval_native_php_type_spec(get_type, false),
        (None, Some(set_type)) => eval_native_php_type_spec(set_type, false),
        _ => None,
    }
}

/// Collects AOT property defaults whose value can be exposed to eval reflection.
pub(super) fn eval_native_property_default_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativePropertyDefaultRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        let default_context = EvalNativeDefaultContext::for_class(ctx.module, class_name);
        collect_eval_native_instance_property_defaults(
            class_name,
            class_info,
            &default_context,
            &mut registrations,
        );
        collect_eval_native_static_property_defaults(
            class_name,
            class_info,
            &default_context,
            &mut registrations,
        );
    }
    registrations
}

/// Collects the array-valued class and interface constants eval reads through a class name.
///
/// Every class registers the constants it can read -- its own, its ancestors' and its
/// interfaces' -- so the interpreter's lookup is one probe; a parent's private constant is not
/// inherited and is left out. Scalars stay with the generated helper.
pub(super) fn eval_native_class_constant_default_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeClassConstantDefaultRegistration> {
    let module = ctx.module;
    let mut registrations = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, _) in classes {
        let mut names = Vec::new();
        collect_eval_class_constant_names(module, class_name, &mut names, 0);
        for constant_name in names {
            let Some((declaring, expr)) =
                eval_native_class_constant_expr(module, class_name, &constant_name)
            else {
                continue;
            };
            let visibility = eval_native_constant_visibility(module, declaring, &constant_name);
            if visibility == Visibility::Private && !declaring.eq_ignore_ascii_case(class_name) {
                continue;
            }
            push_eval_native_array_constant(
                module,
                class_name,
                &constant_name,
                declaring,
                visibility,
                expr,
                &mut registrations,
            );
        }
    }
    let mut interfaces = module.interface_infos.iter().collect::<Vec<_>>();
    interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    for (interface_name, interface_info) in interfaces {
        let mut names = interface_info.constants.keys().cloned().collect::<Vec<_>>();
        names.sort();
        for constant_name in names {
            let Some((declaring, expr)) =
                eval_native_interface_constant_expr(module, interface_name, &constant_name)
            else {
                continue;
            };
            push_eval_native_array_constant(
                module,
                interface_name,
                &constant_name,
                declaring,
                Visibility::Public,
                expr,
                &mut registrations,
            );
        }
    }
    registrations
}

/// Adds one constant when its value is an array the eval default encoding can describe.
fn push_eval_native_array_constant(
    module: &Module,
    class_name: &str,
    constant_name: &str,
    declaring: &str,
    visibility: Visibility,
    expr: &Expr,
    registrations: &mut Vec<EvalNativeClassConstantDefaultRegistration>,
) {
    let default_context = EvalNativeDefaultContext::for_class(module, declaring);
    let Some(default @ EvalNativeCallableDefault::Array(_)) =
        eval_native_callable_default(expr, &default_context)
    else {
        return;
    };
    registrations.push(EvalNativeClassConstantDefaultRegistration {
        class_name: class_name.to_string(),
        constant_name: constant_name.to_string(),
        declaring_class: declaring.to_string(),
        visibility,
        default,
    });
}

/// Collects the constant names one class can read: its own, then its parent's, then its
/// interfaces', without duplicates.
fn collect_eval_class_constant_names(
    module: &Module,
    class_name: &str,
    names: &mut Vec<String>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    let Some((_, class_info)) = resolve_eval_native_default_class(module, class_name) else {
        return;
    };
    for name in &class_info.constant_order {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    for interface_name in &class_info.interfaces {
        if let Some((_, interface_info)) =
            resolve_eval_native_default_interface(module, interface_name)
        {
            let mut interface_names = interface_info.constants.keys().cloned().collect::<Vec<_>>();
            interface_names.sort();
            for name in interface_names {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    if let Some(parent) = class_info.parent.as_deref() {
        collect_eval_class_constant_names(module, parent, names, depth + 1);
    }
}

/// Returns the declared visibility of a class-like constant (interfaces are always public).
fn eval_native_constant_visibility(module: &Module, declaring: &str, constant_name: &str) -> Visibility {
    resolve_eval_native_default_class(module, declaring)
        .and_then(|(_, class_info)| class_info.constant_visibilities.get(constant_name).cloned())
        .unwrap_or(Visibility::Public)
}

/// Collects AOT member attributes whose metadata can be exposed to eval reflection.
pub(super) fn eval_native_member_attribute_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeMemberAttributeRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_eval_native_class_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_method_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_parameter_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_property_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_class_constant_attributes(class_name, class_info, &mut registrations);
    }
    let mut registrations = dedupe_eval_native_member_attribute_registrations(registrations);
    // Free functions have no inherited duplicates; repeatable attributes must retain every entry.
    for function in ctx.module.functions.iter().filter(|function| function_has_eval_metadata(function)) {
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_FUNCTION, &function.name, "",
            &function.attribute_names, &function.attribute_args, &mut registrations,
        );
    }
    registrations
}

/// Removes inherited duplicate member-attribute registrations by normalized metadata key.
pub(super) fn dedupe_eval_native_member_attribute_registrations(
    registrations: Vec<EvalNativeMemberAttributeRegistration>,
) -> Vec<EvalNativeMemberAttributeRegistration> {
    let mut seen = crate::fast_hash::FastSet::default();
    let mut unique = Vec::with_capacity(registrations.len());
    for registration in registrations {
        let key = (
            registration.owner_kind,
            php_symbol_key(&registration.class_name),
            registration.member_name.clone(),
            registration.attribute_name.clone(),
            registration.attribute_args.clone(),
        );
        if seen.insert(key) {
            unique.push(registration);
        }
    }
    unique
}
