//! Purpose:
//! Finds class metadata referenced by static property and static method EIR instructions.
//!
//! Called from:
//! - `super::classes::runtime_referenced_class_names()`.
//! - Program shutdown and web request reset for shared static-property ownership.
//!
//! Key details:
//! - Resolves self, parent, and late-static receivers against the current method owner.

use super::*;

/// Enumerates emitted owning static-property slots once per declaring class, in initializer order.
pub(in crate::codegen) fn refcounted_static_properties(module: &Module) -> Vec<(String, PhpType)> {
    let mut class_names = super::runtime_referenced_class_names(module).into_iter().collect::<Vec<_>>();
    class_names.sort();
    let mut props = Vec::new();
    for class_name in class_names {
        let Some(class_info) = module.class_infos.get(&class_name) else { continue; };
        for (property, php_type) in &class_info.static_properties {
            let declaring_class = class_info.static_property_declaring_classes.get(property)
                .map(String::as_str).unwrap_or(class_name.as_str());
            if declaring_class != class_name { continue; }
            let ty = php_type.codegen_repr();
            if matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted() {
                props.push((crate::names::static_property_symbol(&class_name, property), php_type.clone()));
            }
        }
    }
    props
}

/// Returns class names encoded in static property load/store immediates.
pub(in crate::codegen) fn referenced_static_property_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(
                inst.op,
                Op::LoadStaticProperty
                    | Op::StoreStaticProperty
                    | Op::StaticPropInitialized
                    | Op::LoadReflectionStaticProperty
                    | Op::ReflectionStaticPropertyInitialized
                    | Op::StoreReflectionStaticProperty
            ) {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((class_name, _)) = label.rsplit_once("::") else {
                continue;
            };
            if let Some(class_name) =
                resolve_static_property_metadata_class(module, function, class_name)
            {
                names.insert(class_name);
            }
            if class_name.trim_start_matches('\\') == "static" {
                names.extend(redeclared_late_static_property_classes(
                    module, function, label,
                ));
            }
        }
    }
    names
}

/// Resolves lexical static-property receivers for runtime metadata collection.
pub(in crate::codegen) fn resolve_static_property_metadata_class(
    module: &Module,
    function: &Function,
    class_name: &str,
) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    match class_name {
        "self" => current_function_class(function).map(str::to_string),
        "parent" => {
            let current = current_function_class(function)?;
            module.class_infos.get(current)?.parent.clone()
        }
        "static" => current_function_class(function).map(str::to_string),
        _ => Some(class_name.to_string()),
    }
}

/// Returns descendant classes that redeclare a late-bound static property label.
pub(in crate::codegen) fn redeclared_late_static_property_classes(
    module: &Module,
    function: &Function,
    label: &str,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(base_class) = current_function_class(function) else {
        return names;
    };
    let Some((_, property)) = label.rsplit_once("::") else {
        return names;
    };
    let Some(base_info) = module.class_infos.get(base_class) else {
        return names;
    };
    let fallback_declaring_class = base_info
        .static_property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(base_class);
    for (class_name, class_info) in &module.class_infos {
        if !is_same_or_descendant(module, class_name, base_class) {
            continue;
        }
        let Some(declaring_class) = class_info.static_property_declaring_classes.get(property)
        else {
            continue;
        };
        if declaring_class != fallback_declaring_class {
            names.insert(declaring_class.clone());
        }
    }
    names
}

/// Returns class names encoded in static-method call immediates.
pub(in crate::codegen) fn referenced_static_method_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::StaticMethodCall) {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((class_name, _)) = label.rsplit_once("::") else {
                continue;
            };
            if let Some(class_name) =
                resolve_static_method_metadata_class(module, function, class_name)
            {
                names.insert(class_name);
            }
        }
    }
    names
}

/// Resolves lexical static-method receivers for runtime metadata collection.
pub(in crate::codegen) fn resolve_static_method_metadata_class(
    module: &Module,
    function: &Function,
    class_name: &str,
) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    match class_name {
        "self" | "static" => current_function_class(function).map(str::to_string),
        "parent" => {
            let current = current_function_class(function)?;
            module.class_infos.get(current)?.parent.clone()
        }
        _ => Some(class_name.to_string()),
    }
}

/// Returns true when `class_name` is `ancestor` or one of its descendants.
pub(in crate::codegen) fn is_same_or_descendant(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns the class encoded in an EIR method function name.
pub(in crate::codegen) fn current_function_class(function: &Function) -> Option<&str> {
    function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
}
