//! Purpose:
//! Emits the closed-world class/member registry backing dynamic `method_exists()` and
//! `property_exists()` probes.
//!
//! Called from:
//! - `crate::codegen::emit_program()` when class-method introspection is required.
//!
//! Key details:
//! - Rows are 64 bytes: class name plus class-string method, object method, and property lists.
//! - Method lists are ASCII-lowercased because PHP method names are case-insensitive.
//! - Object method lists include inherited private methods; class-string lists hide them.

use std::collections::{hash_map::Entry, BTreeSet, HashMap, HashSet};

use crate::ir::Module;
use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::ClassInfo;

use super::instanceof::escaped_ascii;

/// One class row and its three PHP-observable member sets.
struct MemberRow {
    sort_key: String,
    class_methods: Vec<String>,
    object_methods: Vec<String>,
    properties: Vec<String>,
}

/// Emits `_member_exists_table` and its count symbol for the checked module.
pub(crate) fn emit_member_exists_registry_data(module: &Module) -> String {
    let rows = member_rows(module);
    let mut out = String::new();
    out.push_str(".data\n.p2align 3\n");
    emit_member_lists(&mut out, &rows);
    emit_member_table(&mut out, &rows);
    out
}

/// Builds deterministic registry rows for every non-synthetic declared class.
fn member_rows(module: &Module) -> Vec<MemberRow> {
    let class_index = canonical_class_index(module);
    let mut rows = module
        .class_infos
        .iter()
        .filter(|(name, _)| !is_internal_synthetic_class_name(name))
        .map(|(class_name, class_info)| {
            let class_method_candidates = class_method_candidates(class_info);
            let object_method_candidates = object_method_candidates(class_info, &class_index);
            let property_candidates = property_candidates(class_info);
            MemberRow {
                sort_key: php_symbol_key(class_name.trim_start_matches('\\')),
                class_methods: class_method_candidates
                .iter()
                .filter(|method| {
                    method_exists_for_class(
                        &class_index,
                        class_name,
                        class_info,
                        method,
                        false,
                    )
                })
                .cloned()
                .collect(),
                object_methods: object_method_candidates
                .iter()
                .filter(|method| {
                    method_exists_for_class(
                        &class_index,
                        class_name,
                        class_info,
                        method,
                        true,
                    )
                })
                .cloned()
                .collect(),
                properties: property_candidates
                .iter()
                .filter(|property| property_exists_for_class(class_name, class_info, property))
                .cloned()
                .collect(),
            }
        })
        .collect::<Vec<_>>();
    // A callable value is a `Closure` object, which no program declares: `method_exists($cb,
    // '__invoke')` over a boxed closure queries this row (twig-bridge's `DebugCommand`).
    if !rows.iter().any(|row| row.sort_key == "closure") {
        let methods: Vec<String> = ["__invoke", "bind", "bindto", "call", "fromcallable"]
            .iter()
            .map(|method| method.to_string())
            .collect();
        rows.push(MemberRow {
            sort_key: "closure".to_string(),
            class_methods: methods.clone(),
            object_methods: methods,
            properties: Vec::new(),
        });
    }
    rows.sort_by(|left, right| left.sort_key.as_bytes().cmp(right.sort_key.as_bytes()));
    rows
}

/// Builds deterministic case-insensitive lookup metadata for every declared class.
fn canonical_class_index(module: &Module) -> HashMap<String, &ClassInfo> {
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
    let mut index = HashMap::with_capacity(classes.len());
    for (raw_name, class_info) in classes {
        let key = php_symbol_key(raw_name.trim_start_matches('\\'));
        match index.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(class_info);
            }
            Entry::Occupied(entry) => {
                debug_assert!(
                    false,
                    "duplicate canonical class metadata key: {}",
                    entry.key()
                );
            }
        }
    }
    index
}

/// Collects method keys that can appear in one class's flattened metadata.
fn class_method_candidates(class_info: &ClassInfo) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.extend(class_info.methods.keys().map(|name| php_symbol_key(name)));
    names.extend(
        class_info
            .static_methods
            .keys()
            .map(|name| php_symbol_key(name)),
    );
    names
}

/// Collects one class's method keys plus keys declared by every reachable ancestor.
fn object_method_candidates(
    class_info: &ClassInfo,
    class_index: &HashMap<String, &ClassInfo>,
) -> BTreeSet<String> {
    let mut names = class_method_candidates(class_info);
    let mut visited = HashSet::new();
    let mut parent_name = class_info.parent.as_deref();
    while let Some(candidate) = parent_name {
        let key = php_symbol_key(candidate.trim_start_matches('\\'));
        if !visited.insert(key.clone()) {
            break;
        }
        let Some(parent_info) = class_index.get(&key).copied() else {
            break;
        };
        names.extend(parent_info.methods.keys().map(|name| php_symbol_key(name)));
        names.extend(
            parent_info
                .static_methods
                .keys()
                .map(|name| php_symbol_key(name)),
        );
        parent_name = parent_info.parent.as_deref();
    }
    names
}

/// Collects one class's case-sensitive instance and static property names.
fn property_candidates(class_info: &ClassInfo) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.extend(class_info.property_visibilities.keys().cloned());
    names.extend(class_info.static_property_visibilities.keys().cloned());
    names
}

/// Returns PHP's method-existence answer for one class, target form, and method key.
fn method_exists_for_class(
    class_index: &HashMap<String, &ClassInfo>,
    class_name: &str,
    class_info: &ClassInfo,
    method_key: &str,
    target_is_object: bool,
) -> bool {
    if class_info.methods.contains_key(method_key)
        && (target_is_object
            || method_visible_from_class_string(
                class_name,
                method_key,
                &class_info.method_visibilities,
                &class_info.method_declaring_classes,
            ))
    {
        return true;
    }
    if class_info.static_methods.contains_key(method_key)
        && (target_is_object
            || method_visible_from_class_string(
                class_name,
                method_key,
                &class_info.static_method_visibilities,
                &class_info.static_method_declaring_classes,
            ))
    {
        return true;
    }
    target_is_object && parent_chain_declares_method(class_index, class_info, method_key)
}

/// Returns whether a method remains visible through a class-string target.
fn method_visible_from_class_string<S1, S2>(
    class_name: &str,
    method_key: &str,
    visibilities: &std::collections::HashMap<String, Visibility, S1>,
    declaring_classes: &std::collections::HashMap<String, String, S2>,
) -> bool
where
    S1: std::hash::BuildHasher,
    S2: std::hash::BuildHasher,
{
    visibilities.get(method_key) != Some(&Visibility::Private)
        || declaring_classes
            .get(method_key)
            .is_none_or(|declaring_class| php_symbol_key(declaring_class) == php_symbol_key(class_name))
}

/// Returns whether any ancestor declares a method, including private methods visible on objects.
fn parent_chain_declares_method(
    class_index: &HashMap<String, &ClassInfo>,
    class_info: &ClassInfo,
    method_key: &str,
) -> bool {
    let mut visited = HashSet::new();
    let mut parent_name = class_info.parent.as_deref();
    while let Some(candidate) = parent_name {
        let key = php_symbol_key(candidate.trim_start_matches('\\'));
        if !visited.insert(key.clone()) {
            return false;
        }
        let Some(parent_info) = class_index.get(&key).copied() else {
            return false;
        };
        if parent_info.methods.contains_key(method_key)
            || parent_info.static_methods.contains_key(method_key)
        {
            return true;
        }
        parent_name = parent_info.parent.as_deref();
    }
    false
}

/// Returns PHP's property-existence answer for one flattened class schema.
fn property_exists_for_class(
    class_name: &str,
    class_info: &ClassInfo,
    property_name: &str,
) -> bool {
    property_visible_from_class_string(
        class_name,
        property_name,
        &class_info.property_visibilities,
        &class_info.property_declaring_classes,
    ) || property_visible_from_class_string(
        class_name,
        property_name,
        &class_info.static_property_visibilities,
        &class_info.static_property_declaring_classes,
    )
}

/// Returns whether a property exists without exposing an inherited private declaration.
fn property_visible_from_class_string<S1, S2>(
    class_name: &str,
    property_name: &str,
    visibilities: &std::collections::HashMap<String, Visibility, S1>,
    declaring_classes: &std::collections::HashMap<String, String, S2>,
) -> bool
where
    S1: std::hash::BuildHasher,
    S2: std::hash::BuildHasher,
{
    let Some(visibility) = visibilities.get(property_name) else {
        return false;
    };
    visibility != &Visibility::Private
        || declaring_classes
            .get(property_name)
            .is_none_or(|declaring_class| php_symbol_key(declaring_class) == php_symbol_key(class_name))
}

/// Emits all non-empty `{name_ptr, name_len}` member lists referenced by registry rows.
fn emit_member_lists(out: &mut String, rows: &[MemberRow]) {
    for (row_index, row) in rows.iter().enumerate() {
        emit_name_bytes(out, &format!("_memex_name_{}", row_index), &row.sort_key);
        emit_one_member_list(out, row_index, "cm", &row.class_methods);
        emit_one_member_list(out, row_index, "om", &row.object_methods);
        emit_one_member_list(out, row_index, "pr", &row.properties);
    }
}

/// Emits one row-owned member list, omitting storage for an empty list.
fn emit_one_member_list(out: &mut String, row_index: usize, kind: &str, names: &[String]) {
    for (slot, name) in names.iter().enumerate() {
        emit_name_bytes(
            out,
            &format!("_memex_{}_{}_{}", kind, row_index, slot),
            name,
        );
    }
    if names.is_empty() {
        return;
    }
    out.push_str(".p2align 3\n");
    out.push_str(&format!("_memex_{}_list_{}:\n", kind, row_index));
    for (slot, name) in names.iter().enumerate() {
        out.push_str(&format!(
            "    .quad _memex_{}_{}_{}\n    .quad {}\n",
            kind,
            row_index,
            slot,
            name.len()
        ));
    }
}

/// Emits the fixed-width registry table and its row-count symbol.
fn emit_member_table(out: &mut String, rows: &[MemberRow]) {
    out.push_str(".p2align 3\n.globl _member_exists_table\n_member_exists_table:\n");
    for (index, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "    .quad _memex_name_{}\n    .quad {}\n",
            index,
            row.sort_key.len()
        ));
        emit_list_ref(out, index, "cm", row.class_methods.len());
        emit_list_ref(out, index, "om", row.object_methods.len());
        emit_list_ref(out, index, "pr", row.properties.len());
    }
    out.push_str(&format!(
        ".p2align 3\n.globl _member_exists_table_count\n_member_exists_table_count:\n    .quad {}\n",
        rows.len()
    ));
}

/// Emits one list pointer/count pair inside a registry row.
fn emit_list_ref(out: &mut String, row_index: usize, kind: &str, count: usize) {
    if count == 0 {
        out.push_str("    .quad 0\n    .quad 0\n");
    } else {
        out.push_str(&format!(
            "    .quad _memex_{}_list_{}\n    .quad {}\n",
            kind, row_index, count
        ));
    }
}

/// Emits one byte-string label consumed with an explicit length word.
fn emit_name_bytes(out: &mut String, label: &str, value: &str) {
    out.push_str(&format!("{}:\n", label));
    out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(value)));
}

/// Returns true for compiler-internal classes excluded from PHP introspection.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::types::{ClassInfo, FunctionSig, PhpType};

    use super::{
        method_exists_for_class, object_method_candidates, parent_chain_declares_method,
    };

    /// Builds an inert function signature for method-membership tests.
    fn empty_function_sig() -> FunctionSig {
        FunctionSig {
            params: Vec::new(),
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Mixed,
            declared_return: false,
            by_ref_return: false,
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
            deprecation: None,
            is_generator: false,
        }
    }

    /// Builds minimal class metadata with the requested parent and instance methods.
    fn class_info(parent: Option<&str>, method_names: &[&str]) -> ClassInfo {
        let methods = method_names
            .iter()
            .map(|name| ((*name).to_string(), empty_function_sig()))
            .collect();
        ClassInfo {
            class_id: 0,
            declaration_span: crate::span::Span::dummy(),
            doc_comment: None,
            parent: parent.map(str::to_string),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            allow_dynamic_properties: false,
            eval_property_storage: false,
            clone_override_property_storage: false,
            scope_dynamic_property_storage: false,
            is_internal: false,
            constants: HashMap::new(),
            constant_order: Vec::new(),
            constant_deprecations: HashMap::new(),
            constant_types: HashMap::new(),
            constant_visibilities: HashMap::new(),
            final_constants: HashSet::new(),
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            constant_attribute_names: HashMap::new(),
            constant_attribute_args: HashMap::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            deferred_property_default_error: None,
            property_visibilities: HashMap::new(),
            property_set_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            property_declared_slots: Vec::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            lazily_unset_properties: HashSet::new(),
            backed_hooked_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            owned_reference_properties: HashSet::new(),
            promoted_properties: HashSet::new(),
            property_reference_slots: Vec::new(),
            property_slot_declaring_classes: Vec::new(),
            property_owned_reference_slots: Vec::new(),
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            property_hooks: HashMap::new(),
            reflection_properties: Vec::new(),
            reflection_methods: Vec::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods,
            static_methods: crate::fast_hash::FastMap::default(),
            late_static_method_returns: HashMap::new(),
            late_static_static_method_returns: HashMap::new(),
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: crate::fast_hash::FastMap::default(),
            final_methods: HashSet::new(),
            method_declaring_classes: crate::fast_hash::FastMap::default(),
            method_impl_classes: crate::fast_hash::FastMap::default(),
            abstract_methods: HashSet::new(),
            vtable_methods: Vec::new(),
            vtable_slots: HashMap::new(),
            static_method_visibilities: crate::fast_hash::FastMap::default(),
            final_static_methods: HashSet::new(),
            static_method_declaring_classes: crate::fast_hash::FastMap::default(),
            static_method_impl_classes: crate::fast_hash::FastMap::default(),
            abstract_static_methods: HashSet::new(),
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            declared_interfaces: Vec::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        }
    }

    /// Pins reachable-before-missing, beyond-missing, and cyclic parent frontiers.
    #[test]
    fn parent_method_lookup_preserves_malformed_metadata_frontiers() {
        let child = class_info(Some("Mid"), &[]);
        let mid = class_info(Some("Missing"), &["before_missing"]);
        let cycle_a = class_info(Some("CycleB"), &[]);
        let cycle_b = class_info(Some("CycleA"), &[]);
        let class_index = HashMap::from([
            ("child".to_string(), &child),
            ("mid".to_string(), &mid),
            ("cyclea".to_string(), &cycle_a),
            ("cycleb".to_string(), &cycle_b),
        ]);

        let candidates = object_method_candidates(&child, &class_index);
        assert!(candidates.contains("before_missing"));
        assert!(method_exists_for_class(
            &class_index,
            "Child",
            &child,
            "before_missing",
            true,
        ));
        assert!(!method_exists_for_class(
            &class_index,
            "Child",
            &child,
            "beyond_missing",
            true,
        ));
        assert!(!parent_chain_declares_method(
            &class_index,
            &cycle_a,
            "absent",
        ));
    }
}
