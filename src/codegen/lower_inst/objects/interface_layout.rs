//! Purpose:
//! Validates interface method symbols and computes object initialization offsets.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Only layouts representable by emitted EIR method metadata are accepted.

use super::*;

/// Returns true when implemented interfaces require method-wrapper metadata not emitted here.
pub(super) fn class_interfaces_require_missing_method_symbols(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
) -> bool {
    let emitted_methods = emitted_instance_method_keys(ctx);
    let mut seen = HashSet::new();
    let mut stack = class_info
        .interfaces
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
            return true;
        };
        if interface_requires_missing_method_symbol(
            ctx,
            interface_name,
            class_name,
            class_info,
            interface_info,
            &emitted_methods,
        ) {
            return true;
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
    false
}

/// Returns true when one interface table entry would point at an unavailable symbol.
pub(super) fn interface_requires_missing_method_symbol(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    fallback_class: &str,
    class_info: &ClassInfo,
    interface_info: &InterfaceInfo,
    emitted_methods: &HashSet<(String, String)>,
) -> bool {
    for method_name in &interface_info.method_order {
        if super::super::is_throwable_standard_method_key(method_name)
            && (super::super::is_throwable_like_class(ctx, fallback_class)
                || super::super::is_throwable_like_class(ctx, interface_name))
        {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(method_name)
            .map(String::as_str)
            .unwrap_or(fallback_class);
        let key = (impl_class.to_string(), method_name.clone());
        if !emitted_methods.contains(&key)
            && IntrinsicCall::instance_method(impl_class, method_name)
                .and_then(|intrinsic| intrinsic.runtime_helper())
                .is_none()
        {
            return true;
        }
    }
    false
}

/// Returns instance-method keys emitted by the EIR backend.
pub(super) fn emitted_instance_method_keys(ctx: &FunctionContext<'_>) -> HashSet<(String, String)> {
    ctx.module
        .class_methods
        .iter()
        .filter(|function| !function.flags.is_static)
        .filter_map(|function| {
            let (class_name, method_name) = function.name.rsplit_once("::")?;
            Some((class_name.to_string(), php_symbol_key(method_name)))
        })
        .collect()
}

/// Returns true when the current EIR module includes a class method body.
pub(super) fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_symbol_key(candidate_method) == method_key
                })
    })
}

/// Collects property high-word offsets that should start with the typed-property sentinel.
pub(super) fn uninitialized_property_marker_offsets(class_info: &ClassInfo) -> Vec<usize> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, _))| {
            let is_owned_reference = class_info.owned_reference_properties.contains(property)
                && class_info.property_slot_is_reference(index, property);
            let starts_uninitialized = class_info.property_slot_is_declared(index, property)
                && class_info.defaults.get(index).is_some_and(|default| default.is_none())
                && !is_owned_reference;
            if starts_uninitialized {
                Some(8 + index * 16 + 8)
            } else {
                None
            }
        })
        .collect()
}

/// Collects the slot offsets of object-owned reference properties, whose ref-cells the
/// object allocates at construction and releases on destruction.
pub(super) fn owned_reference_property_offsets(class_info: &ClassInfo) -> Vec<(usize, PhpType)> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, php_type))| {
            if class_info.owned_reference_properties.contains(property)
                && class_info.property_slot_is_reference(index, property)
            {
                Some((8 + index * 16, php_type.clone()))
            } else {
                None
            }
        })
        .collect()
}
