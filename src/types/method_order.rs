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
//!   own declarations. Through PHP 8.4 trait methods are bound after inheritance, so a class's
//!   table is its own methods, then its parent's table, then the methods its traits
//!   contribute. PHP 8.5 binds traits first: own methods, trait methods, then the parent's.
//! - A class's own declaration of a trait method's name overrides it and keeps the class's
//!   position (see [`trait_imported_method_keys`]).
//! - A key keeps its first position; keys are lowercase php symbol keys.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::span::Span;
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

/// Finds `name`'s class info, falling back to a case-insensitive php symbol-key match.
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

/// Appends `class_name`'s table: its own declarations, then (version-dependent order) its
/// trait-imported methods and its parent's table, each key at its first position.
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
    let traits_before_parent = crate::codegen_support::compile_php_version()
        >= crate::php_version::PhpVersion::Php85;
    if traits_before_parent {
        append_trait_methods(&declared, &from_traits, order, seen);
    }
    if let Some(parent) = info.parent.as_deref() {
        if php_symbol_key(parent) != php_symbol_key(class_name) {
            append_class_table(parent, classes, imported, order, seen, depth + 1);
        }
    }
    if !traits_before_parent {
        append_trait_methods(&declared, &from_traits, order, seen);
    }
}

/// Appends the class's trait-imported method keys, in declaration order, that have no
/// position yet.
fn append_trait_methods(
    declared: &[String],
    from_traits: &HashSet<String>,
    order: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    for key in declared.iter().filter(|key| from_traits.contains(*key)) {
        if seen.insert(key.clone()) {
            order.push(key.clone());
        }
    }
}

/// The method keys `info` imported from its traits: methods its used traits (directly or
/// through nested `use` inside a trait) contribute, plus trait aliases.
///
/// A trait method the class overrides with its own declaration is NOT imported: Zend keeps the
/// class's declaration in its own position. `span_of` returns a trait method's declaration
/// span; trait composition copies that declaration into the class unchanged (an alias too,
/// under its new name), so a class method is imported exactly when its span is some trait
/// method's span. That one test covers class overrides of trait methods and of aliases, and
/// methods a nested trait contributes. A synthetic span (built prelude code) identifies no
/// declaration, so such a method falls back to matching the names its direct traits declare
/// and the class's aliases.
pub(crate) fn trait_imported_method_keys<V>(
    info: &ClassInfo,
    declared_trait_methods: &HashMap<String, HashMap<String, V>>,
    span_of: &dyn Fn(&V) -> Span,
) -> HashSet<String> {
    let trait_spans: HashSet<Span> = declared_trait_methods
        .values()
        .flat_map(|methods| methods.values().map(span_of))
        .filter(|span| span.identifies_a_node())
        .collect();
    let mut by_name = HashSet::new();
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
            by_name.extend(methods.keys().map(|name| php_symbol_key(name)));
        }
    }
    by_name.extend(info.trait_aliases.iter().map(|(alias, _)| php_symbol_key(alias)));
    info.method_decls
        .iter()
        .filter(|decl| {
            if decl.span.identifies_a_node() {
                trait_spans.contains(&decl.span)
            } else {
                by_name.contains(&php_symbol_key(&decl.name))
            }
        })
        .map(|decl| php_symbol_key(&decl.name))
        .collect()
}
