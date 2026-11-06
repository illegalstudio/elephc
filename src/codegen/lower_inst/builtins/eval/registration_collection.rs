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
pub(super) fn eval_native_function_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeFunctionRegistration> {
    ctx.module
        .functions
        .iter()
        .filter(|function| function_has_eval_metadata(function))
        .map(|function| EvalNativeFunctionRegistration {
            name: function.name.clone(),
            signature: function_signature_from_eir(function),
            bridge_supported: function_signature_can_bridge_with_eval(function),
            is_internal: function.flags.is_internal,
        })
        .collect()
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
    let mut seen = std::collections::HashSet::new();
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
