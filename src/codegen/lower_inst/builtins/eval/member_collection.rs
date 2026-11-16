//! Purpose:
//! Collects class hierarchy, member, property, and method metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - PHP-style name resolution and inherited declaring-class rules are preserved.

use super::*;

/// Registers generated AOT class parent metadata for eval `parent::` resolution.
pub(super) fn register_eval_native_class_parents(ctx: &mut FunctionContext<'_>, context_source: EvalContextSource) {
    let mut parents = ctx
        .module
        .class_infos
        .iter()
        .filter_map(|(class_name, class_info)| {
            let parent_name = class_info.parent.as_deref()?;
            Some((
                class_info.class_id,
                class_name.clone(),
                parent_name.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    parents.sort_by_key(|(class_id, _, _)| *class_id);
    for (_, class_name, parent_name) in parents {
        register_eval_native_class_parent(ctx, context_source, &class_name, &parent_name);
    }
}

/// Adds class-level attribute metadata for one class-like symbol to eval registration.
pub(super) fn collect_eval_native_class_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    collect_eval_native_member_attributes(
        NATIVE_MEMBER_ATTRIBUTE_CLASS,
        class_name,
        "",
        &class_info.attribute_names,
        &class_info.attribute_args,
        registrations,
    );
}

/// Adds method attribute metadata for one class to eval registration.
pub(super) fn collect_eval_native_method_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    let mut methods = class_info.method_attribute_names.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method_name, _)| method_name.as_str());
    for (method_name, attribute_names) in methods {
        let attribute_args = class_info
            .method_attribute_args
            .get(method_name)
            .cloned()
            .unwrap_or_default();
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_METHOD,
            eval_native_method_declaring_class(class_name, class_info, method_name),
            method_name,
            attribute_names,
            &attribute_args,
            registrations,
        );
    }
}

/// Adds property attribute metadata for one class to eval registration.
pub(super) fn collect_eval_native_property_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    let mut properties = class_info
        .property_attribute_names
        .iter()
        .collect::<Vec<_>>();
    properties.sort_by_key(|(property_name, _)| property_name.as_str());
    for (property_name, attribute_names) in properties {
        let attribute_args = class_info
            .property_attribute_args
            .get(property_name)
            .cloned()
            .unwrap_or_default();
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_PROPERTY,
            eval_native_property_attribute_declaring_class(class_name, class_info, property_name),
            property_name,
            attribute_names,
            &attribute_args,
            registrations,
        );
    }
}

/// Adds class-constant attribute metadata for one class to eval registration.
pub(super) fn collect_eval_native_class_constant_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    let mut constants = class_info
        .constant_attribute_names
        .iter()
        .collect::<Vec<_>>();
    constants.sort_by_key(|(constant_name, _)| constant_name.as_str());
    for (constant_name, attribute_names) in constants {
        let attribute_args = class_info
            .constant_attribute_args
            .get(constant_name)
            .cloned()
            .unwrap_or_default();
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT,
            class_name,
            constant_name,
            attribute_names,
            &attribute_args,
            registrations,
        );
    }
}

/// Adds aligned attribute name/argument metadata for one AOT member.
pub(super) fn collect_eval_native_member_attributes(
    owner_kind: u8,
    class_name: &str,
    member_name: &str,
    attribute_names: &[String],
    attribute_args: &[Option<Vec<AttrArgEntry>>],
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    for (index, attribute_name) in attribute_names.iter().enumerate() {
        let Some(args) = attribute_args.get(index).cloned().flatten() else {
            continue;
        };
        let attribute_args = if eval_native_member_attribute_args_supported(&args) {
            Some(args)
        } else {
            None
        };
        registrations.push(EvalNativeMemberAttributeRegistration {
            owner_kind,
            class_name: class_name.to_string(),
            member_name: member_name.to_string(),
            attribute_name: attribute_name.clone(),
            attribute_args,
        });
    }
}

/// Adds supported instance-property default metadata for one class to eval registration.
pub(super) fn collect_eval_native_instance_property_defaults(
    class_name: &str,
    class_info: &ClassInfo,
    default_context: &EvalNativeDefaultContext<'_>,
    registrations: &mut Vec<EvalNativePropertyDefaultRegistration>,
) {
    for (slot, (property_name, _)) in class_info.properties.iter().enumerate() {
        let default = class_info.defaults.get(slot).and_then(Option::as_ref);
        let is_declared = class_info.property_slot_is_declared(slot, property_name);
        let is_abstract = class_info.abstract_properties.contains(property_name);
        let Some(default) =
            eval_native_property_default(default, is_declared, is_abstract, default_context)
        else {
            continue;
        };
        registrations.push(EvalNativePropertyDefaultRegistration {
            class_name: eval_native_instance_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            default,
        });
    }
}

/// Adds supported static-property default metadata for one class to eval registration.
pub(super) fn collect_eval_native_static_property_defaults(
    class_name: &str,
    class_info: &ClassInfo,
    default_context: &EvalNativeDefaultContext<'_>,
    registrations: &mut Vec<EvalNativePropertyDefaultRegistration>,
) {
    for (slot, (property_name, _)) in class_info.static_properties.iter().enumerate() {
        let default = class_info
            .static_defaults
            .get(slot)
            .and_then(Option::as_ref);
        let is_declared = class_info
            .declared_static_properties
            .contains(property_name);
        let Some(default) =
            eval_native_property_default(default, is_declared, false, default_context)
        else {
            continue;
        };
        registrations.push(EvalNativePropertyDefaultRegistration {
            class_name: eval_native_static_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            default,
        });
    }
}

/// Adds declared instance-property type metadata for one class to eval registration.
pub(super) fn collect_eval_native_instance_property_types(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativePropertyTypeRegistration>,
) {
    for (slot, (property_name, php_type)) in class_info.properties.iter().enumerate() {
        if !class_info.property_slot_is_declared(slot, property_name) {
            continue;
        }
        let Some(type_spec) = eval_native_php_type_spec(php_type, false) else {
            continue;
        };
        registrations.push(EvalNativePropertyTypeRegistration {
            class_name: eval_native_instance_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            type_spec,
        });
    }
}

/// Adds declared static-property type metadata for one class to eval registration.
pub(super) fn collect_eval_native_static_property_types(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativePropertyTypeRegistration>,
) {
    for (property_name, php_type) in &class_info.static_properties {
        if !class_info
            .declared_static_properties
            .contains(property_name)
        {
            continue;
        }
        let Some(type_spec) = eval_native_php_type_spec(php_type, false) else {
            continue;
        };
        registrations.push(EvalNativePropertyTypeRegistration {
            class_name: eval_native_static_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            type_spec,
        });
    }
}

/// Returns the class name that declares one AOT instance property row.
pub(super) fn eval_native_instance_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one AOT static property row.
pub(super) fn eval_native_static_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .static_property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one AOT method metadata row, reading the declaring-class
/// maps before the implementing ones so attributes are keyed by the class eval reflection reports.
pub(super) fn eval_native_method_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    method_name: &str,
) -> &'a str {
    class_info
        .method_declaring_classes
        .get(method_name)
        .or_else(|| class_info.static_method_declaring_classes.get(method_name))
        .or_else(|| class_info.method_impl_classes.get(method_name))
        .or_else(|| class_info.static_method_impl_classes.get(method_name))
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one AOT property attribute row.
pub(super) fn eval_native_property_attribute_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .property_declaring_classes
        .get(property_name)
        .or_else(|| {
            class_info
                .static_property_declaring_classes
                .get(property_name)
        })
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Adds instance method metadata for one class to eval signature registration.
pub(super) fn collect_eval_native_instance_methods(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = class_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        if method_name == "__construct" {
            continue;
        }
        let bridge_supported = class_method_visibility_bridge_supported(class_info, method_name)
            && method_signature_can_bridge_with_eval(signature);
        registrations.push(EvalNativeMethodRegistration {
            class_name: class_name.to_string(),
            method_name: method_name.clone(),
            is_static: false,
            signature: signature.clone(),
            bridge_supported,
        });
    }
}

/// Adds static method metadata for one class to eval signature registration.
pub(super) fn collect_eval_native_static_methods(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = class_info.static_methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        let bridge_supported =
            class_static_method_visibility_bridge_supported(class_info, method_name)
                && method_signature_can_bridge_with_eval(signature);
        registrations.push(EvalNativeMethodRegistration {
            class_name: class_name.to_string(),
            method_name: method_name.clone(),
            is_static: true,
            signature: signature.clone(),
            bridge_supported,
        });
    }
}

/// Adds interface instance-method metadata to eval signature registration.
pub(super) fn collect_eval_native_interface_instance_methods(
    interface_name: &str,
    interface_info: &InterfaceInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = interface_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        registrations.push(EvalNativeMethodRegistration {
            class_name: eval_native_interface_method_declaring_interface(
                interface_name,
                interface_info,
                method_name,
            )
            .to_string(),
            method_name: method_name.clone(),
            is_static: false,
            signature: signature.clone(),
            bridge_supported: false,
        });
    }
}

/// Adds interface static-method metadata to eval signature registration.
pub(super) fn collect_eval_native_interface_static_methods(
    interface_name: &str,
    interface_info: &InterfaceInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = interface_info.static_methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        registrations.push(EvalNativeMethodRegistration {
            class_name: eval_native_interface_static_method_declaring_interface(
                interface_name,
                interface_info,
                method_name,
            )
            .to_string(),
            method_name: method_name.clone(),
            is_static: true,
            signature: signature.clone(),
            bridge_supported: false,
        });
    }
}

/// Returns the interface name that declares one AOT interface instance method row.
pub(super) fn eval_native_interface_method_declaring_interface<'a>(
    reflected_interface: &'a str,
    interface_info: &'a InterfaceInfo,
    method_name: &str,
) -> &'a str {
    interface_info
        .method_declaring_interfaces
        .get(method_name)
        .map(String::as_str)
        .unwrap_or(reflected_interface)
}

/// Returns the interface name that declares one AOT interface static method row.
pub(super) fn eval_native_interface_static_method_declaring_interface<'a>(
    reflected_interface: &'a str,
    interface_info: &'a InterfaceInfo,
    method_name: &str,
) -> &'a str {
    interface_info
        .static_method_declaring_interfaces
        .get(method_name)
        .map(String::as_str)
        .unwrap_or(reflected_interface)
}
