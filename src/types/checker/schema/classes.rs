use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{ClassProperty, Visibility};
use crate::types::{ClassInfo, PhpType};
use crate::types::traits::FlattenedClass;

use super::super::{infer_expr_type_syntactic, Checker};
use super::validation::{
    build_constructor_param_map, build_method_sig, validate_override_signature,
    validate_signature_compatibility, visibility_rank,
};

pub(crate) fn build_class_info_recursive(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if checker.classes.contains_key(class_name) {
        return Ok(());
    }

    if !building.insert(class_name.to_string()) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Circular inheritance detected involving class {}",
                class_name
            ),
        ));
    }

    let class = class_map.get(class_name).cloned().ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Unknown class referenced during inheritance flattening: {}",
                class_name
            ),
        )
    })?;

    if class.is_abstract && class.is_final {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "Cannot use the final modifier on an abstract class",
        ));
    }

    let parent_info = if let Some(parent_name) = &class.extends {
        if checker.interfaces.contains_key(parent_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} cannot extend interface {}; use implements instead",
                    class_name, parent_name
                ),
            ));
        }
        build_class_info_recursive(parent_name, class_map, checker, next_class_id, building)?;
        Some(checker.classes.get(parent_name).cloned().ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown parent class: {}", parent_name),
            )
        })?)
    } else {
        None
    };

    if let (Some(parent), Some(parent_name)) = (&parent_info, class.extends.as_ref()) {
        if parent.is_final {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Class {} cannot extend final class {}", class.name, parent_name),
            ));
        }
        if class.is_readonly_class != parent.is_readonly_class {
            let relation = if class.is_readonly_class {
                "readonly class cannot extend non-readonly parent"
            } else {
                "non-readonly class cannot extend readonly parent"
            };
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("{}: {} extends {}", relation, class.name, parent_name),
            ));
        }
    }

    let mut prop_types = Vec::new();
    let mut property_offsets = HashMap::new();
    let mut property_declaring_classes = HashMap::new();
    let mut defaults = Vec::new();
    let mut property_visibilities = HashMap::new();
    let mut declared_properties = HashSet::new();
    let mut final_properties = HashSet::new();
    let mut readonly_properties = std::collections::HashSet::new();
    let mut reference_properties = HashSet::new();
    let mut static_prop_types = Vec::new();
    let mut static_defaults = Vec::new();
    let mut static_property_declaring_classes = HashMap::new();
    let mut static_property_visibilities = HashMap::new();
    let mut declared_static_properties = HashSet::new();
    let mut final_static_properties = HashSet::new();

    let mut method_sigs = HashMap::new();
    let mut static_sigs = HashMap::new();
    let mut method_visibilities = HashMap::new();
    let mut final_methods = HashSet::new();
    let mut method_declaring_classes = HashMap::new();
    let mut method_impl_classes = HashMap::new();
    let mut vtable_methods = Vec::new();
    let mut vtable_slots = HashMap::new();
    let mut static_method_visibilities = HashMap::new();
    let mut final_static_methods = HashSet::new();
    let mut static_method_declaring_classes = HashMap::new();
    let mut static_method_impl_classes = HashMap::new();
    let mut static_vtable_methods = Vec::new();
    let mut static_vtable_slots = HashMap::new();
    let mut interfaces = Vec::new();

    if let Some(parent) = &parent_info {
        for (index, (name, ty)) in parent.properties.iter().enumerate() {
            prop_types.push((name.clone(), ty.clone()));
            property_offsets.insert(name.clone(), 8 + index * 16);
            defaults.push(parent.defaults[index].clone());
            if let Some(visibility) = parent.property_visibilities.get(name) {
                property_visibilities.insert(name.clone(), visibility.clone());
            }
            if parent.declared_properties.contains(name) {
                declared_properties.insert(name.clone());
            }
            if let Some(declaring_class) = parent.property_declaring_classes.get(name) {
                property_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if parent.final_properties.contains(name) {
                final_properties.insert(name.clone());
            }
            if parent.readonly_properties.contains(name) {
                readonly_properties.insert(name.clone());
            }
            if parent.reference_properties.contains(name) {
                reference_properties.insert(name.clone());
            }
        }
        for (index, (name, ty)) in parent.static_properties.iter().enumerate() {
            static_prop_types.push((name.clone(), ty.clone()));
            static_defaults.push(parent.static_defaults[index].clone());
            if let Some(visibility) = parent.static_property_visibilities.get(name) {
                static_property_visibilities.insert(name.clone(), visibility.clone());
            }
            if parent.declared_static_properties.contains(name) {
                declared_static_properties.insert(name.clone());
            }
            if let Some(declaring_class) = parent.static_property_declaring_classes.get(name) {
                static_property_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if parent.final_static_properties.contains(name) {
                final_static_properties.insert(name.clone());
            }
        }

        for (name, sig) in &parent.methods {
            if parent.method_visibilities.get(name) == Some(&Visibility::Private) {
                continue;
            }
            method_sigs.insert(name.clone(), sig.clone());
            if let Some(visibility) = parent.method_visibilities.get(name) {
                method_visibilities.insert(name.clone(), visibility.clone());
            }
            if parent.final_methods.contains(name) {
                final_methods.insert(name.clone());
            }
            if let Some(declaring_class) = parent.method_declaring_classes.get(name) {
                method_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if let Some(impl_class) = parent.method_impl_classes.get(name) {
                method_impl_classes.insert(name.clone(), impl_class.clone());
            }
        }
        vtable_methods = parent.vtable_methods.clone();
        vtable_slots = parent.vtable_slots.clone();

        for (name, sig) in &parent.static_methods {
            if parent.static_method_visibilities.get(name) == Some(&Visibility::Private) {
                continue;
            }
            static_sigs.insert(name.clone(), sig.clone());
            if let Some(visibility) = parent.static_method_visibilities.get(name) {
                static_method_visibilities.insert(name.clone(), visibility.clone());
            }
            if parent.final_static_methods.contains(name) {
                final_static_methods.insert(name.clone());
            }
            if let Some(declaring_class) = parent.static_method_declaring_classes.get(name) {
                static_method_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if let Some(impl_class) = parent.static_method_impl_classes.get(name) {
                static_method_impl_classes.insert(name.clone(), impl_class.clone());
            }
        }
        static_vtable_methods = parent.static_vtable_methods.clone();
        static_vtable_slots = parent.static_vtable_slots.clone();
        interfaces = parent.interfaces.clone();
    }

    for prop in &class.properties {
        if prop.is_static {
            if prop.by_ref {
                return Err(CompileError::new(
                    prop.span,
                    "Static by-reference properties are not supported",
                ));
            }
            if property_declaring_classes.contains_key(&prop.name) {
                return Err(CompileError::new(
                    prop.span,
                    &format!(
                        "Cannot redeclare instance property as static property: {}::{}",
                        class.name, prop.name
                    ),
                ));
            }
            if static_property_declaring_classes.contains_key(&prop.name) {
                if final_static_properties.contains(&prop.name) {
                    let declaring_class = static_property_declaring_classes
                        .get(&prop.name)
                        .cloned()
                        .unwrap_or_else(|| class.name.clone());
                    return Err(CompileError::new(
                        prop.span,
                        &format!(
                            "Cannot override final static property {}::${}",
                            declaring_class, prop.name
                        ),
                    ));
                }
                return Err(CompileError::new(
                    prop.span,
                    &format!(
                        "Static property redeclaration across inheritance is not yet supported: {}::{}",
                        class.name, prop.name
                    ),
                ));
            }

            let ty = if let Some(declared_ty) = resolve_property_declared_type(checker, &class.name, prop)? {
                checker.validate_declared_default_type(
                    &declared_ty,
                    prop.default.as_ref(),
                    prop.span,
                    &format!("Static property {}::${} default", class.name, prop.name),
                )?;
                declared_static_properties.insert(prop.name.clone());
                declared_ty
            } else if let Some(default) = &prop.default {
                infer_expr_type_syntactic(default)
            } else {
                PhpType::Int
            };
            static_prop_types.push((prop.name.clone(), ty));
            static_defaults.push(prop.default.clone());
            static_property_declaring_classes.insert(prop.name.clone(), class.name.clone());
            static_property_visibilities.insert(prop.name.clone(), prop.visibility.clone());
            if prop.is_final {
                final_static_properties.insert(prop.name.clone());
            } else {
                final_static_properties.remove(&prop.name);
            }
            continue;
        }
        if prop.is_final && prop.visibility == Visibility::Private {
            return Err(CompileError::new(
                prop.span,
                "Property cannot be both final and private",
            ));
        }
        if static_property_declaring_classes.contains_key(&prop.name) {
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Cannot redeclare static property as instance property: {}::{}",
                    class.name, prop.name
                ),
            ));
        }
        if prop.by_ref && class.is_readonly_class {
            return Err(CompileError::new(
                prop.span,
                "Readonly promoted by-reference properties are not supported",
            ));
        }
        if property_declaring_classes.contains_key(&prop.name) {
            if final_properties.contains(&prop.name) {
                let declaring_class = property_declaring_classes
                    .get(&prop.name)
                    .cloned()
                    .unwrap_or_else(|| class.name.clone());
                return Err(CompileError::new(
                    prop.span,
                    &format!(
                        "Cannot override final property {}::${}",
                        declaring_class, prop.name
                    ),
                ));
            }
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Property redeclaration across inheritance is not yet supported: {}::{}",
                    class.name, prop.name
                ),
            ));
        }

        let ty = if let Some(declared_ty) = resolve_property_declared_type(checker, &class.name, prop)? {
            checker.validate_declared_default_type(
                &declared_ty,
                prop.default.as_ref(),
                prop.span,
                &format!("Property {}::${} default", class.name, prop.name),
            )?;
            declared_properties.insert(prop.name.clone());
            declared_ty
        } else if let Some(default) = &prop.default {
            infer_expr_type_syntactic(default)
        } else {
            PhpType::Int
        };
        let slot_index = prop_types.len();
        prop_types.push((prop.name.clone(), ty));
        property_offsets.insert(prop.name.clone(), 8 + slot_index * 16);
        property_declaring_classes.insert(prop.name.clone(), class.name.clone());
        defaults.push(prop.default.clone());
        property_visibilities.insert(prop.name.clone(), prop.visibility.clone());
        if prop.is_final {
            final_properties.insert(prop.name.clone());
        } else {
            final_properties.remove(&prop.name);
        }
        if class.is_readonly_class || prop.readonly {
            readonly_properties.insert(prop.name.clone());
        }
        if prop.by_ref {
            reference_properties.insert(prop.name.clone());
        }
    }

    for method in &class.methods {
        let sig = build_method_sig(checker, method)?;
        if method.is_abstract && method.is_final {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Cannot use the final modifier on an abstract method: {}::{}",
                    class.name, method.name
                ),
            ));
        }
        if method.is_abstract && method.has_body {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Abstract method cannot have a body: {}::{}",
                    class.name, method.name
                ),
            ));
        }
        if !method.is_abstract && !method.has_body {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Non-abstract method must have a body: {}::{}",
                    class.name, method.name
                ),
            ));
        }
        if method.is_abstract && method.visibility == Visibility::Private {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Private abstract methods are not supported: {}::{}",
                    class.name, method.name
                ),
            ));
        }
        if method.is_static {
            if final_methods.contains(&method.name) {
                let declaring_class = method_declaring_classes
                    .get(&method.name)
                    .cloned()
                    .unwrap_or_else(|| class.name.clone());
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot override final method {}::{}",
                        declaring_class, method.name
                    ),
                ));
            }
            if method_sigs.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot change method kind when overriding {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            if final_static_methods.contains(&method.name) {
                let declaring_class = static_method_declaring_classes
                    .get(&method.name)
                    .cloned()
                    .unwrap_or_else(|| class.name.clone());
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot override final method {}::{}",
                        declaring_class, method.name
                    ),
                ));
            }
            if let Some(parent_visibility) = static_method_visibilities.get(&method.name) {
                if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
                    return Err(CompileError::new(
                        method.span,
                        &format!(
                            "Cannot reduce visibility when overriding static method: {}::{}",
                            class.name, method.name
                        ),
                    ));
                }
            }
            if let Some(parent_sig) = static_sigs.get(&method.name) {
                validate_override_signature(checker, &class.name, method, parent_sig, true)?;
            }
            if method.is_abstract && static_method_impl_classes.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot make concrete static method abstract: {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            static_sigs.insert(method.name.clone(), sig);
            static_method_visibilities.insert(method.name.clone(), method.visibility.clone());
            if method.is_final {
                final_static_methods.insert(method.name.clone());
            } else {
                final_static_methods.remove(&method.name);
            }
            static_method_declaring_classes.insert(method.name.clone(), class.name.clone());
            if method.is_abstract {
                static_method_impl_classes.remove(&method.name);
            } else {
                static_method_impl_classes.insert(method.name.clone(), class.name.clone());
            }
            if method.visibility != Visibility::Private
                && !static_vtable_slots.contains_key(&method.name)
            {
                let slot = static_vtable_methods.len();
                static_vtable_slots.insert(method.name.clone(), slot);
                static_vtable_methods.push(method.name.clone());
            }
        } else {
            if final_static_methods.contains(&method.name) {
                let declaring_class = static_method_declaring_classes
                    .get(&method.name)
                    .cloned()
                    .unwrap_or_else(|| class.name.clone());
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot override final method {}::{}",
                        declaring_class, method.name
                    ),
                ));
            }
            if static_sigs.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot change method kind when overriding {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            if final_methods.contains(&method.name) {
                let declaring_class = method_declaring_classes
                    .get(&method.name)
                    .cloned()
                    .unwrap_or_else(|| class.name.clone());
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot override final method {}::{}",
                        declaring_class, method.name
                    ),
                ));
            }
            if let Some(parent_visibility) = method_visibilities.get(&method.name) {
                if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
                    return Err(CompileError::new(
                        method.span,
                        &format!(
                            "Cannot reduce visibility when overriding method: {}::{}",
                            class.name, method.name
                        ),
                    ));
                }
            }
            if let Some(parent_sig) = method_sigs.get(&method.name) {
                validate_override_signature(checker, &class.name, method, parent_sig, false)?;
            }
            if method.is_abstract && method_impl_classes.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot make concrete method abstract: {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            method_sigs.insert(method.name.clone(), sig);
            method_visibilities.insert(method.name.clone(), method.visibility.clone());
            if method.is_final {
                final_methods.insert(method.name.clone());
            } else {
                final_methods.remove(&method.name);
            }
            method_declaring_classes.insert(method.name.clone(), class.name.clone());
            if method.is_abstract {
                method_impl_classes.remove(&method.name);
            } else {
                method_impl_classes.insert(method.name.clone(), class.name.clone());
            }
            if method.visibility != Visibility::Private && !vtable_slots.contains_key(&method.name)
            {
                let slot = vtable_methods.len();
                vtable_slots.insert(method.name.clone(), slot);
                vtable_methods.push(method.name.clone());
            }
        }
    }

    let mut seen_interfaces: HashSet<String> = interfaces.iter().cloned().collect();
    let mut queue = Vec::new();
    for interface_name in class.implements.iter().rev() {
        if class_map.contains_key(interface_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} cannot implement non-interface {}",
                    class.name, interface_name
                ),
            ));
        }
        if !checker.interfaces.contains_key(interface_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            ));
        }
        queue.push(interface_name.clone());
    }
    while let Some(interface_name) = queue.pop() {
        if !seen_interfaces.insert(interface_name.clone()) {
            continue;
        }
        let interface_info = checker.interfaces.get(&interface_name).ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            )
        })?;
        for parent_name in interface_info.parents.iter().rev() {
            queue.push(parent_name.clone());
        }
        interfaces.push(interface_name);
    }

    for interface_name in &interfaces {
        let interface_info = checker.interfaces.get(interface_name).ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            )
        })?;
        for method_name in &interface_info.method_order {
            if static_sigs.contains_key(method_name) {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Cannot use static method to satisfy interface contract: {}::{}",
                        class.name, method_name
                    ),
                ));
            }
            let required_sig = interface_info
                .methods
                .get(method_name)
                .expect("type checker bug: missing interface method signature");
            let actual_sig = match method_sigs.get(method_name) {
                Some(sig) => sig,
                None if class.is_abstract => {
                    method_sigs.insert(method_name.clone(), required_sig.clone());
                    method_visibilities.insert(method_name.clone(), Visibility::Public);
                    method_declaring_classes.insert(method_name.clone(), class.name.clone());
                    method_impl_classes.remove(method_name);
                    if !vtable_slots.contains_key(method_name) {
                        let slot = vtable_methods.len();
                        vtable_slots.insert(method_name.clone(), slot);
                        vtable_methods.push(method_name.clone());
                    }
                    continue;
                }
                None => {
                    return Err(CompileError::new(
                        crate::span::Span::dummy(),
                        &format!(
                            "Class {} must implement interface method {}::{}",
                            class.name, interface_name, method_name
                        ),
                    ))
                }
            };
            validate_signature_compatibility(
                crate::span::Span::dummy(),
                &class.name,
                method_name,
                actual_sig,
                required_sig,
                "method",
                "implementing interface",
            )?;
            if method_visibilities.get(method_name) != Some(&Visibility::Public) {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Interface method implementation must be public: {}::{}",
                        class.name, method_name
                    ),
                ));
            }
        }
    }

    if !class.is_abstract {
        if let Some(method_name) = method_sigs
            .keys()
            .find(|name| !method_impl_classes.contains_key(*name))
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Concrete class {} must implement abstract method {}::{}",
                    class.name, class.name, method_name
                ),
            ));
        }
        if let Some(method_name) = static_sigs
            .keys()
            .find(|name| !static_method_impl_classes.contains_key(*name))
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Concrete class {} must implement abstract static method {}::{}",
                    class.name, class.name, method_name
                ),
            ));
        }
    }

    let constructor_param_to_prop = if class.methods.iter().any(|m| m.name == "__construct") {
        build_constructor_param_map(&class.methods)
    } else if let Some(parent) = &parent_info {
        parent.constructor_param_to_prop.clone()
    } else {
        Vec::new()
    };

    checker.classes.insert(
        class.name.clone(),
        ClassInfo {
            class_id: *next_class_id,
            parent: class.extends.clone(),
            is_abstract: class.is_abstract,
            is_final: class.is_final,
            is_readonly_class: class.is_readonly_class,
            properties: prop_types,
            property_offsets,
            property_declaring_classes,
            defaults,
            property_visibilities,
            declared_properties,
            final_properties,
            readonly_properties,
            reference_properties,
            static_properties: static_prop_types,
            static_defaults,
            static_property_declaring_classes,
            static_property_visibilities,
            declared_static_properties,
            final_static_properties,
            method_decls: class.methods.clone(),
            methods: method_sigs,
            static_methods: static_sigs,
            method_visibilities,
            final_methods,
            method_declaring_classes,
            method_impl_classes,
            vtable_methods,
            vtable_slots,
            static_method_visibilities,
            final_static_methods,
            static_method_declaring_classes,
            static_method_impl_classes,
            static_vtable_methods,
            static_vtable_slots,
            interfaces,
            constructor_param_to_prop,
        },
    );
    *next_class_id += 1;
    building.remove(class_name);
    Ok(())
}

fn resolve_property_declared_type(
    checker: &Checker,
    class_name: &str,
    prop: &ClassProperty,
) -> Result<Option<PhpType>, CompileError> {
    prop.type_expr
        .as_ref()
        .map(|type_expr| {
            checker.resolve_declared_property_type_hint(
                type_expr,
                prop.span,
                &format!("Property {}::${}", class_name, prop.name),
            )
        })
        .transpose()
}
