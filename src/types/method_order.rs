//! Purpose:
//! Zend's function-table order for a class's methods, shared by reflection
//! (`ReflectionClass::getMethods()`) and `get_class_methods()`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection::class_members`.
//! - `crate::ir_lower::expr::reflection_filters` and `crate::ir_lower::expr::class_introspection`.
//!
//! Key details:
//! - zend_do_inheritance appends the parent's (not overridden) functions after the class's
//!   own declarations, and trait methods are bound after inheritance, so a class's table is
//!   its own methods, then its parent's table, then the methods its traits contribute.
//! - A key keeps its first position; keys are lowercase php symbol keys.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::types::ClassInfo;

/// Returns `class_name`'s method keys in Zend's function-table order. `imported` names the
/// method keys a class imported from its traits (including aliases).
pub(crate) fn zend_method_table_order(
    class_name: &str,
    classes: &HashMap<String, ClassInfo>,
    imported: &dyn Fn(&ClassInfo) -> HashSet<String>,
) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    append_class_table(class_name, classes, imported, &mut order, &mut seen, 0);
    order
}

fn lookup<'a>(classes: &'a HashMap<String, ClassInfo>, name: &str) -> Option<&'a ClassInfo> {
    let trimmed = name.trim_start_matches('\\');
    classes.get(trimmed).or_else(|| {
        let key = php_symbol_key(trimmed);
        classes
            .iter()
            .find(|(candidate, _)| php_symbol_key(candidate) == key)
            .map(|(_, info)| info)
    })
}

fn append_class_table(
    class_name: &str,
    classes: &HashMap<String, ClassInfo>,
    imported: &dyn Fn(&ClassInfo) -> HashSet<String>,
    order: &mut Vec<String>,
    seen: &mut HashSet<String>,
    depth: usize,
) {
    let Some(info) = lookup(classes, class_name) else {
        return;
    };
    if depth > 64 {
        return;
    }
    let from_traits = imported(info);
    let declared: Vec<String> = info
        .method_decls
        .iter()
        .map(|method| php_symbol_key(&method.name))
        .collect();
    for key in declared.iter().filter(|key| !from_traits.contains(*key)) {
        if seen.insert(key.clone()) {
            order.push(key.clone());
        }
    }
    if let Some(parent) = info.parent.as_deref() {
        if php_symbol_key(parent) != php_symbol_key(class_name) {
            append_class_table(parent, classes, imported, order, seen, depth + 1);
        }
    }
    for key in declared.iter().filter(|key| from_traits.contains(*key)) {
        if seen.insert(key.clone()) {
            order.push(key.clone());
        }
    }
}

/// The method keys `info` imported from its traits, from each used trait's declared
/// methods plus the class's trait aliases.
pub(crate) fn trait_imported_method_keys<V>(
    info: &ClassInfo,
    declared_trait_methods: &HashMap<String, HashMap<String, V>>,
) -> HashSet<String> {
    let mut keys = HashSet::new();
    for trait_name in &info.used_traits {
        let trimmed = trait_name.trim_start_matches('\\');
        let methods = declared_trait_methods.get(trimmed).or_else(|| {
            let key = php_symbol_key(trimmed);
            declared_trait_methods
                .iter()
                .find(|(candidate, _)| php_symbol_key(candidate) == key)
                .map(|(_, methods)| methods)
        });
        if let Some(methods) = methods {
            keys.extend(methods.keys().map(|name| php_symbol_key(name)));
        }
    }
    for (alias, _) in &info.trait_aliases {
        keys.insert(php_symbol_key(alias));
    }
    keys
}
