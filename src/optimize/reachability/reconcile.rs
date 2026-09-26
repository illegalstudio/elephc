//! Purpose:
//! Reconciles type-checker declaration metadata after AST reachability pruning.
//! Keeps method maps and vtable layouts aligned with the declarations EIR can lower.
//!
//! Called from:
//! - `crate::optimize::reachability::prune_unreachable_declarations()`.
//!
//! Key details:
//! - Synthetic checker-only classes are preserved unless backed by a pruned source declaration.
//! - Vtable survivor order is stable and slots are compacted from zero.
//! - Shared virtual methods retain identical slots along every live inheritance lineage.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::types::{CheckResult, ClassInfo};

use super::graph::{ClassKind, DeclarationIndex, Reachability};

/// Mutates checker metadata so every declaration table agrees with the pruned AST.
pub(super) fn check_result(
    check: &mut CheckResult,
    reachability: &Reachability,
    declarations: &DeclarationIndex,
    original_builtin_libraries: &HashSet<String>,
    remaining_builtin_libraries: &HashSet<String>,
) {
    let declared_extern_libraries: HashSet<String> = check
        .extern_functions
        .iter()
        .filter(|(name, _)| declarations.externs.contains(&php_symbol_key(name)))
        .filter_map(|(_, signature)| signature.library.clone())
        .collect();
    retain_free_function_metadata(check, reachability, declarations);
    retain_class_metadata(check, reachability, declarations);
    retain_extern_metadata(check, reachability, declarations);
    retain_callable_parameter_metadata(check, reachability, declarations);
    recompute_required_libraries(
        check,
        &declared_extern_libraries,
        original_builtin_libraries,
        remaining_builtin_libraries,
    );
}

/// Removes signatures and attributes for pruned source functions while preserving builtins.
fn retain_free_function_metadata(
    check: &mut CheckResult,
    reachability: &Reachability,
    declarations: &DeclarationIndex,
) {
    check.functions.retain(|name, _| {
        let key = php_symbol_key(name);
        (!declarations.functions.contains_key(&key) && !declarations.externs.contains(&key))
            || reachability.functions.contains(&key)
            || reachability.externs.contains(&key)
    });
    retain_function_keyed_map(
        &mut check.function_attribute_names,
        &reachability.functions,
        &declarations.functions,
    );
    retain_function_keyed_map(
        &mut check.function_attribute_args,
        &reachability.functions,
        &declarations.functions,
    );
    retain_function_keyed_map(
        &mut check.callable_return_sigs,
        &reachability.functions,
        &declarations.functions,
    );
    retain_function_keyed_map(
        &mut check.callable_array_return_sigs,
        &reachability.functions,
        &declarations.functions,
    );
}

/// Filters a function-keyed checker map only when its key belongs to a source declaration.
fn retain_function_keyed_map<T>(
    map: &mut HashMap<String, T>,
    reachable: &HashSet<String>,
    declared: &HashMap<String, super::usage::Usage>,
) {
    map.retain(|name, _| {
        let key = php_symbol_key(name);
        !declared.contains_key(&key) || reachable.contains(&key)
    });
}

/// Removes pruned class-like schemas and filters method metadata on live source classes.
fn retain_class_metadata(
    check: &mut CheckResult,
    reachability: &Reachability,
    declarations: &DeclarationIndex,
) {
    check.classes.retain(|name, _| {
        let key = php_symbol_key(name);
        !declarations.classes.contains_key(&key) || reachability.classes.contains(&key)
    });
    for (name, class_info) in &mut check.classes {
        let key = php_symbol_key(name);
        if declarations.classes.contains_key(&key) {
            prune_class_methods(&key, class_info, reachability);
        }
    }
    #[cfg(debug_assertions)]
    assert_inherited_vtable_slots_aligned(&check.classes);
    #[cfg(debug_assertions)]
    assert_instantiation_vtable_slots_aligned(&check.classes);
    check.interfaces.retain(|name, _| {
        let key = php_symbol_key(name);
        !matches!(
            declarations.classes.get(&key).map(|node| node.kind),
            Some(ClassKind::Interface)
        ) || reachability.classes.contains(&key)
    });
    check.enums.retain(|name, _| {
        let key = php_symbol_key(name);
        !matches!(
            declarations.classes.get(&key).map(|node| node.kind),
            Some(ClassKind::Enum)
        ) || reachability.classes.contains(&key)
    });
    check.packed_classes.retain(|name, _| {
        let key = php_symbol_key(name);
        !declarations.packed_classes.contains(&key) || reachability.classes.contains(&key)
    });
}

/// Asserts two instantiations of one template number their shared methods identically.
///
/// Variance relates `Box<Dog>` to `Box<Animal>` with no inheritance edge between them, so the
/// assertion above cannot see the relation and cannot fire on it. A widened call takes the slot
/// from the EXPECTED instantiation and indexes the ACTUAL one's table, which is only sound while
/// the two agree.
///
/// They agreed by construction until pruning recompacted each class independently: `Box<Animal>`,
/// named in a parameter type but never constructed, lost the `__construct` that `Box<Dog>` kept,
/// and `whoAmI` moved from slot 2 to slot 1 in one of them. The call then landed on `get`, whose
/// returned object was read as a string — silently, exit code 0. `prune_class_methods` now keeps
/// every method NAME on an instantiated class for exactly this reason; this is the guard that
/// says so out loud if that ever stops being true.
#[cfg(debug_assertions)]
fn assert_instantiation_vtable_slots_aligned(classes: &HashMap<String, ClassInfo>) {
    // `Box<int>` and `Box<string>` share the template `box`; an ordinary class has no `<` and
    // is never grouped with anything.
    let mut by_template: HashMap<&str, Vec<(&String, &ClassInfo)>> = HashMap::new();
    for (name, info) in classes {
        let Some(template) = name.split_once('<').map(|(head, _)| head) else {
            continue;
        };
        by_template.entry(template).or_default().push((name, info));
    }
    for (template, siblings) in by_template {
        let Some(((first_name, first), rest)) = siblings.split_first() else {
            continue;
        };
        for (other_name, other) in rest {
            for (method, slot) in &first.vtable_slots {
                if let Some(other_slot) = other.vtable_slots.get(method) {
                    debug_assert_eq!(
                        slot, other_slot,
                        "instance vtable slot for {method} diverged between {first_name} and \
                         {other_name}, two instantiations of {template}: a widened call takes the \
                         slot from one and indexes the other",
                    );
                }
            }
            for (method, slot) in &first.static_vtable_slots {
                if let Some(other_slot) = other.static_vtable_slots.get(method) {
                    debug_assert_eq!(
                        slot, other_slot,
                        "static vtable slot for {method} diverged between {first_name} and \
                         {other_name}, two instantiations of {template}",
                    );
                }
            }
        }
    }
}

/// Asserts shared virtual methods retain one slot number across each live inheritance edge.
#[cfg(debug_assertions)]
fn assert_inherited_vtable_slots_aligned(classes: &HashMap<String, ClassInfo>) {
    let by_key: HashMap<_, _> = classes
        .iter()
        .map(|(name, info)| (php_symbol_key(name), info))
        .collect();
    for (class_name, child) in classes {
        let Some(parent_name) = child.parent.as_deref() else {
            continue;
        };
        let Some(parent) = by_key.get(&php_symbol_key(parent_name)).copied() else {
            continue;
        };
        for (method, parent_slot) in &parent.vtable_slots {
            if let Some(child_slot) = child.vtable_slots.get(method) {
                debug_assert_eq!(
                    child_slot, parent_slot,
                    "instance vtable slot for {class_name}::{method} diverged from {parent_name}",
                );
            }
        }
        for (method, parent_slot) in &parent.static_vtable_slots {
            if let Some(child_slot) = child.static_vtable_slots.get(method) {
                debug_assert_eq!(
                    child_slot, parent_slot,
                    "static vtable slot for {class_name}::{method} diverged from {parent_name}",
                );
            }
        }
    }
}

/// Filters every instance/static method map and rebuilds stable compact vtable slots.
fn prune_class_methods(
    class_key: &str,
    info: &mut ClassInfo,
    reachability: &Reachability,
) {
    let instance_impl = info.method_impl_classes.clone();
    let instance_declaring = info.method_declaring_classes.clone();
    let static_impl = info.static_method_impl_classes.clone();
    let static_declaring = info.static_method_declaring_classes.clone();
    let keep_instance: HashSet<String> = info
        .methods
        .keys()
        .filter(|method| {
            method_is_live(
                class_key,
                method,
                false,
                &instance_impl,
                &instance_declaring,
                reachability,
            )
        })
        .cloned()
        .collect();
    let keep_static: HashSet<String> = info
        .static_methods
        .keys()
        .filter(|method| {
            method_is_live(
                class_key,
                method,
                true,
                &static_impl,
                &static_declaring,
                reachability,
            )
        })
        .cloned()
        .collect();
    // A private override can remain as an inherited vtable slot marker even though
    // descendants do not inherit its method metadata. Preserve graph-selected
    // markers independently so compacting a descendant cannot shift later slots.
    let reachable_instance_slots: HashSet<&str> = reachability
        .methods
        .iter()
        .filter_map(|(class, method, is_static)| {
            (class == class_key && !*is_static).then_some(method.as_str())
        })
        .collect();
    let reachable_static_slots: HashSet<&str> = reachability
        .methods
        .iter()
        .filter_map(|(class, method, is_static)| {
            (class == class_key && *is_static).then_some(method.as_str())
        })
        .collect();
    let keep_any: HashSet<String> = keep_instance.union(&keep_static).cloned().collect();

    info.method_decls.retain(|method| {
        let key = php_symbol_key(&method.name);
        if method.is_static {
            keep_static.contains(&key)
        } else {
            keep_instance.contains(&key)
        }
    });
    retain_keys(&mut info.methods, &keep_instance);
    retain_keys(&mut info.late_static_method_returns, &keep_instance);
    retain_keys(&mut info.callable_method_return_sigs, &keep_any);
    retain_keys(&mut info.callable_array_method_return_sigs, &keep_any);
    retain_keys(&mut info.method_visibilities, &keep_instance);
    info.final_methods.retain(|key| keep_instance.contains(key));
    retain_keys(&mut info.method_declaring_classes, &keep_instance);
    retain_keys(&mut info.method_impl_classes, &keep_instance);
    info.abstract_methods.retain(|key| keep_instance.contains(key));

    retain_keys(&mut info.static_methods, &keep_static);
    retain_keys(&mut info.late_static_static_method_returns, &keep_static);
    retain_keys(&mut info.static_method_visibilities, &keep_static);
    info.final_static_methods.retain(|key| keep_static.contains(key));
    retain_keys(&mut info.static_method_declaring_classes, &keep_static);
    retain_keys(&mut info.static_method_impl_classes, &keep_static);
    info.abstract_static_methods.retain(|key| keep_static.contains(key));

    info.method_attribute_names
        .retain(|key, _| keep_any.contains(key));
    info.method_attribute_args
        .retain(|key, _| keep_any.contains(key));

    // An INSTANTIATION must number its slots exactly like its siblings, and compacting each one
    // independently pulls them apart. A widened call site computes the slot from one
    // instantiation and indexes ANOTHER's table: `inspect(Box<Animal> $b)` reached by a
    // `Box<Dog>` takes `whoAmI`'s number out of `Box<Animal>`. `Box<Animal>` is never
    // constructed, so its `__construct` is not live, so compaction moves `whoAmI` from slot 2 to
    // slot 1 — and the call lands on `Box<Dog>::get`, whose returned object is then read as a
    // string. Silently.
    //
    // Keeping the NAME keeps the number. The body is still pruned: with no entry in
    // `method_impl_classes` the emitter writes a null into that slot, and nothing ever
    // dispatches through the hole, because a receiver that reaches it carries the real entry.
    // `assert_inherited_vtable_slots_aligned` guards the same property across an inheritance
    // edge; variance relates two classes with no such edge, which is why it needs its own.
    let instantiated = class_key.contains('<');
    info.vtable_methods.retain(|key| {
        instantiated
            || keep_instance.contains(key)
            || reachable_instance_slots.contains(key.as_str())
    });
    info.vtable_slots = compact_slots(&info.vtable_methods);
    info.static_vtable_methods.retain(|key| {
        instantiated || keep_static.contains(key) || reachable_static_slots.contains(key.as_str())
    });
    info.static_vtable_slots = compact_slots(&info.static_vtable_methods);
}

/// Returns whether a method is reachable through its visible, implementing, or declaring class.
fn method_is_live(
    class_key: &str,
    method: &str,
    is_static: bool,
    implementations: &HashMap<String, String>,
    declaring: &HashMap<String, String>,
    reachability: &Reachability,
) -> bool {
    let visible = (
        class_key.to_string(),
        method.to_string(),
        is_static,
    );
    if reachability.methods.contains(&visible) {
        return true;
    }
    implementations
        .get(method)
        .into_iter()
        .chain(declaring.get(method))
        .any(|owner| {
            reachability.methods.contains(&(
                php_symbol_key(owner),
                method.to_string(),
                is_static,
            ))
        })
}

/// Retains string-keyed map entries selected by one canonical method keep-set.
fn retain_keys<T>(map: &mut HashMap<String, T>, keep: &HashSet<String>) {
    map.retain(|key, _| keep.contains(key));
}

/// Rebuilds vtable slots from survivor order without sorting or leaving gaps.
fn compact_slots(methods: &[String]) -> HashMap<String, usize> {
    methods
        .iter()
        .enumerate()
        .map(|(slot, method)| (method.clone(), slot))
        .collect()
}

/// Removes pruned FFI function/class schemas while leaving globals conservative.
fn retain_extern_metadata(
    check: &mut CheckResult,
    reachability: &Reachability,
    declarations: &DeclarationIndex,
) {
    check.extern_functions.retain(|name, _| {
        let key = php_symbol_key(name);
        !declarations.externs.contains(&key) || reachability.externs.contains(&key)
    });
    check.extern_classes.retain(|name, _| {
        let key = php_symbol_key(name);
        !declarations.extern_classes.contains(&key) || reachability.classes.contains(&key)
    });
}

/// Drops callable-parameter refinements owned by pruned functions or methods.
fn retain_callable_parameter_metadata(
    check: &mut CheckResult,
    reachability: &Reachability,
    declarations: &DeclarationIndex,
) {
    check.callable_param_sigs.retain(|(owner, _), _| {
        if let Some((class, method)) = owner.rsplit_once("::") {
            let class_key = php_symbol_key(class);
            let method_key = php_symbol_key(method);
            if declarations.classes.contains_key(&class_key) {
                return reachability
                    .methods
                    .iter()
                    .any(|(kept_class, kept_method, _)| {
                        kept_class == &class_key && kept_method == &method_key
                    });
            }
            return true;
        }
        let key = php_symbol_key(owner);
        !declarations.functions.contains_key(&key) || reachability.functions.contains(&key)
    });
}

/// Removes libraries contributed exclusively by declarations and builtin calls that no longer survive.
fn recompute_required_libraries(
    check: &mut CheckResult,
    declared_extern_libraries: &HashSet<String>,
    original_builtin_libraries: &HashSet<String>,
    remaining_builtin_libraries: &HashSet<String>,
) {
    let remaining_extern_libraries: HashSet<String> = check
        .extern_functions
        .values()
        .filter_map(|signature| signature.library.clone())
        .collect();
    check.required_libraries.retain(|library| {
        let removed_builtin_requirement = original_builtin_libraries.contains(library)
            && !remaining_builtin_libraries.contains(library)
            && !remaining_extern_libraries.contains(library);
        let removed_extern_requirement = declared_extern_libraries.contains(library)
            && !remaining_extern_libraries.contains(library)
            && !remaining_builtin_libraries.contains(library);
        !removed_builtin_requirement && !removed_extern_requirement
    });
}
