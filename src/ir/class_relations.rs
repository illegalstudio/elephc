//! Purpose:
//! The names `class_implements`, `class_parents` and `class_uses` answer, computed from a
//! `Module` alone — the closed-world class metadata both backends already carry.
//!
//! Called from:
//! - `codegen::lower_inst::builtins::class_relations` — the native lowering, which emits the
//!   resulting hash through the arch-specific assembler.
//! - `codegen_wasm::builtins` — the WASM lowering, which folds the same names into a hash
//!   built at compile time.
//!
//! Key details:
//! - This lives here, shared, because the ANSWER is a property of the module, not of a backend.
//!   Two copies would be two things to keep in step, and a divergence would be silent: each
//!   backend would emit a well-formed hash, just not the same one.
//! - PHP resolves class-like names written as strings case-insensitively and ignoring a leading
//!   `\`, so every lookup folds both sides before comparing.
//! - ORDER is observable — `foreach` walks these hashes in insertion order — so each list is
//!   built in PHP's order: declared interfaces as written, ancestors from the immediate parent
//!   upward, traits as used.

use std::collections::HashMap;

use crate::ir::Module;
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, InterfaceInfo};

/// What a class-relation argument resolved to. The four namespaces answer differently, and
/// `Unknown` is not an error: PHP returns `false` for a name it cannot resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassLikeTarget {
    /// A declared class or enum.
    Class(String),
    /// A declared interface.
    Interface(String),
    /// A declared trait.
    Trait(String),
    /// A name this module never declares.
    Unknown,
}

/// Which relation is being asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassRelation {
    /// `class_implements`: interfaces of a class, or parent interfaces of an interface.
    Implements,
    /// `class_parents`: ancestors from the immediate parent upward.
    Parents,
    /// `class_uses`: traits used directly.
    Uses,
}

impl ClassRelation {
    /// Maps a PHP builtin name to its relation, or `None` when it is not one of the three.
    pub fn from_builtin_name(name: &str) -> Option<Self> {
        match name {
            "class_implements" => Some(Self::Implements),
            "class_parents" => Some(Self::Parents),
            "class_uses" => Some(Self::Uses),
            _ => None,
        }
    }
}

/// Resolves a class-like name written as a string against this module's declarations.
///
/// The order of the three lookups is PHP's own: a name is a class before it is an interface
/// before it is a trait, and the namespaces are disjoint in practice.
pub fn resolve_named_target(module: &Module, raw: &str) -> ClassLikeTarget {
    if let Some(name) = lookup_class_name(module, raw) {
        return ClassLikeTarget::Class(name);
    }
    if let Some(name) = lookup_interface_name(module, raw) {
        return ClassLikeTarget::Interface(name);
    }
    if let Some(name) = lookup_trait_name(module, raw) {
        return ClassLikeTarget::Trait(name);
    }
    ClassLikeTarget::Unknown
}

/// Resolves the target of a statically known object type.
pub fn resolve_object_target(module: &Module, class_name: &str) -> ClassLikeTarget {
    lookup_class_name(module, class_name)
        .map(ClassLikeTarget::Class)
        .unwrap_or(ClassLikeTarget::Unknown)
}

/// Returns the names one relation answers for `target`, in PHP's iteration order.
pub fn relation_names(
    module: &Module,
    relation: ClassRelation,
    target: &ClassLikeTarget,
) -> Vec<String> {
    match relation {
        ClassRelation::Implements => implements(module, target),
        ClassRelation::Parents => parents(module, target),
        ClassRelation::Uses => uses(module, target),
    }
}

/// Interfaces a class implements, or the parent interfaces of an interface.
fn implements(module: &Module, target: &ClassLikeTarget) -> Vec<String> {
    match target {
        ClassLikeTarget::Class(class_name) => lookup_class(module, class_name)
            .map(|info| info.interfaces.clone())
            .unwrap_or_default(),
        ClassLikeTarget::Interface(interface_name) => {
            let mut names = Vec::new();
            collect_interface_parents(module, interface_name, &mut names);
            names
        }
        ClassLikeTarget::Trait(_) | ClassLikeTarget::Unknown => Vec::new(),
    }
}

/// Ancestors from the immediate parent upward.
fn parents(module: &Module, target: &ClassLikeTarget) -> Vec<String> {
    let ClassLikeTarget::Class(class_name) = target else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut current = class_name.clone();
    while let Some(info) = lookup_class(module, &current) {
        let Some(parent) = &info.parent else {
            break;
        };
        let parent_name = lookup_class_name(module, parent).unwrap_or_else(|| parent.clone());
        names.push(parent_name.clone());
        current = parent_name;
    }
    names
}

/// Traits used directly by a class, or by a trait declaration.
fn uses(module: &Module, target: &ClassLikeTarget) -> Vec<String> {
    match target {
        ClassLikeTarget::Class(class_name) => lookup_class(module, class_name)
            .map(|info| info.used_traits.clone())
            .unwrap_or_default(),
        ClassLikeTarget::Trait(trait_name) => module
            .declared_trait_uses
            .get(trait_name)
            .cloned()
            .unwrap_or_default(),
        ClassLikeTarget::Interface(_) | ClassLikeTarget::Unknown => Vec::new(),
    }
}

/// Walks parent interfaces depth-first, skipping any already collected.
///
/// The dedup is by folded name rather than by identity: a diamond of interfaces reaches the
/// same ancestor twice, and PHP lists it once.
pub fn collect_interface_parents(module: &Module, interface_name: &str, names: &mut Vec<String>) {
    let Some(interface) = lookup_interface(module, interface_name) else {
        return;
    };
    for parent in &interface.parents {
        let parent_name = lookup_interface_name(module, parent).unwrap_or_else(|| parent.clone());
        if !names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            names.push(parent_name.clone());
            collect_interface_parents(module, &parent_name, names);
        }
    }
}

/// Looks up a class (or enum) by PHP-style case-insensitive name.
pub fn lookup_class<'a>(module: &'a Module, name: &str) -> Option<&'a ClassInfo> {
    let name = lookup_class_name(module, name)?;
    module.class_infos.get(&name)
}

/// Looks up an interface by PHP-style case-insensitive name.
pub fn lookup_interface<'a>(module: &'a Module, name: &str) -> Option<&'a InterfaceInfo> {
    let name = lookup_interface_name(module, name)?;
    module.interface_infos.get(&name)
}

/// Returns the declared spelling of a class name, matched PHP-style.
pub fn lookup_class_name(module: &Module, raw: &str) -> Option<String> {
    lookup_folded(module.class_infos.keys(), raw)
}

/// Returns the declared spelling of an interface name, matched PHP-style.
pub fn lookup_interface_name(module: &Module, raw: &str) -> Option<String> {
    lookup_folded(module.interface_infos.keys(), raw)
}

/// Returns the declared spelling of a trait name, matched PHP-style.
pub fn lookup_trait_name(module: &Module, raw: &str) -> Option<String> {
    lookup_folded(module.trait_table.names.iter(), raw)
}

/// Finds a declared name equal to `raw` under PHP's folding rules.
fn lookup_folded<'a>(names: impl Iterator<Item = &'a String>, raw: &str) -> Option<String> {
    let key = php_symbol_key(raw.trim_start_matches('\\'));
    names
        .into_iter()
        .find(|name| php_symbol_key(name.trim_start_matches('\\')) == key)
        .cloned()
}

/// Whether `class_name` implements `interface_name`, following the class's ancestors.
///
/// `ClassInfo::interfaces` already carries the flattened set for a declared class, but a class
/// whose parent declares the interface still has to reach it, so the walk is up the chain.
pub fn class_implements_interface(module: &Module, class_name: &str, interface_name: &str) -> bool {
    let wanted = php_symbol_key(interface_name.trim_start_matches('\\'));
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut current = class_name.to_string();
    while let Some(info) = lookup_class(module, &current) {
        if info
            .interfaces
            .iter()
            .any(|name| php_symbol_key(name.trim_start_matches('\\')) == wanted)
        {
            return true;
        }
        let Some(parent) = &info.parent else {
            return false;
        };
        // A malformed inheritance cycle must not spin here; the checker rejects one, but this
        // walk is also reached from codegen where that guarantee is not re-established.
        if seen.insert(php_symbol_key(parent), ()).is_some() {
            return false;
        }
        current = parent.clone();
    }
    false
}
