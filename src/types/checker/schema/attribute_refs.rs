//! Purpose:
//! Post-pass that drops attribute arguments whose deferred symbolic references
//! (global/class constants, enum cases) cannot be resolved against the complete
//! class/interface/enum tables, restoring "compiles but is not reflectable"
//! behavior for references the EIR backend cannot lower.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl`, after class/interface/enum
//!   metadata is fully built.
//!
//! Key details:
//! - Resolvability is checked exactly the way `ir_lower`'s `lower_scoped_constant`
//!   resolves a `Type::MEMBER` reference (leading-backslash trim, then enum case
//!   lookup, then a class/interface constant chain walk). A ref the lowering
//!   cannot resolve — e.g. the built-in `Attribute::TARGET_CLASS` — would emit an
//!   unsupported `ScopedConstantGet`, so the whole attribute's args are marked
//!   `None` here instead, matching pre-feature behavior.

use std::collections::HashSet;

use crate::types::{AttrArgEntry, AttrArgValue};

use super::super::Checker;

/// Drops every attribute-argument list (class-, method-, and property-level)
/// that contains an unresolvable deferred symbolic reference, setting it to
/// `None`. Runs after all class/interface/enum schemas are built.
pub(crate) fn drop_unresolvable_attribute_arg_refs(checker: &mut Checker) {
    // Phase 1 (immutable): record which (class, level, key, index) arg-lists hold
    // an unresolvable reference. Resolvability reads the class/interface/enum
    // tables, so this must finish before any mutation borrows them mutably.
    let mut class_targets: Vec<(String, usize)> = Vec::new();
    let mut method_targets: Vec<(String, String, usize)> = Vec::new();
    let mut property_targets: Vec<(String, String, usize)> = Vec::new();

    for (class_name, class_info) in &checker.classes {
        for (idx, args) in class_info.attribute_args.iter().enumerate() {
            if arg_list_has_unresolvable_ref(checker, args) {
                class_targets.push((class_name.clone(), idx));
            }
        }
        for (member, lists) in &class_info.method_attribute_args {
            for (idx, args) in lists.iter().enumerate() {
                if arg_list_has_unresolvable_ref(checker, args) {
                    method_targets.push((class_name.clone(), member.clone(), idx));
                }
            }
        }
        for (member, lists) in &class_info.property_attribute_args {
            for (idx, args) in lists.iter().enumerate() {
                if arg_list_has_unresolvable_ref(checker, args) {
                    property_targets.push((class_name.clone(), member.clone(), idx));
                }
            }
        }
    }

    // Phase 2 (mutable): blank out the recorded arg-lists.
    for (class_name, idx) in class_targets {
        if let Some(class_info) = checker.classes.get_mut(&class_name) {
            if let Some(slot) = class_info.attribute_args.get_mut(idx) {
                *slot = None;
            }
        }
    }
    for (class_name, member, idx) in method_targets {
        if let Some(class_info) = checker.classes.get_mut(&class_name) {
            if let Some(lists) = class_info.method_attribute_args.get_mut(&member) {
                if let Some(slot) = lists.get_mut(idx) {
                    *slot = None;
                }
            }
        }
    }
    for (class_name, member, idx) in property_targets {
        if let Some(class_info) = checker.classes.get_mut(&class_name) {
            if let Some(lists) = class_info.property_attribute_args.get_mut(&member) {
                if let Some(slot) = lists.get_mut(idx) {
                    *slot = None;
                }
            }
        }
    }
}

/// Returns true when a captured arg-list resolves to `Some(..)` and any entry
/// (at any array depth) carries a `ScopedConst` reference that cannot be
/// resolved. `None` arg-lists are already unsupported and need no change.
fn arg_list_has_unresolvable_ref(
    checker: &Checker,
    args: &Option<Vec<AttrArgEntry>>,
) -> bool {
    match args {
        Some(entries) => entries
            .iter()
            .any(|entry| value_has_unresolvable_ref(checker, &entry.value)),
        None => false,
    }
}

/// Recursively checks a captured attribute value for a scoped reference the
/// reflection backend cannot materialize. Global `ConstRef`s are always treated
/// as supported: an unknown global constant lowers to a runtime `LoadGlobal`,
/// which the backend supports, so it never crashes the way an unresolvable
/// `ScopedConstantGet` would.
fn value_has_unresolvable_ref(checker: &Checker, value: &AttrArgValue) -> bool {
    match value {
        AttrArgValue::ScopedConst(type_name, member) => {
            !scoped_const_materializable(checker, type_name, member)
        }
        AttrArgValue::Array(entries) => entries
            .iter()
            .any(|entry| value_has_unresolvable_ref(checker, &entry.value)),
        _ => false,
    }
}

/// Returns true when `Type::MEMBER` is a reference reflection can materialize on
/// every supported target: an enum case (resolved to its case object) or a
/// constant reachable on a class (through its parent chain and interfaces) or on
/// an interface.
fn scoped_const_materializable(checker: &Checker, type_name: &str, member: &str) -> bool {
    let normalized = type_name.trim_start_matches('\\');
    if let Some(enum_info) = checker.enums.get(normalized) {
        return enum_info.cases.iter().any(|case| case.name == member);
    }
    class_constant_reachable(checker, normalized, member)
        || interface_constant_reachable(checker, normalized, member, &mut HashSet::new())
}

/// Walks a class's own constants, its parent chain, and the interfaces it
/// implements, looking for `member`. Mirrors `LoweringContext::scoped_constant_value`.
fn class_constant_reachable(checker: &Checker, class_name: &str, member: &str) -> bool {
    let mut current = Some(class_name.to_string());
    let mut guard = 0usize;
    while let Some(name) = current {
        guard += 1;
        if guard > 128 {
            break;
        }
        let Some(class_info) = checker.classes.get(&name) else {
            break;
        };
        if class_info.constants.contains_key(member) {
            return true;
        }
        for interface_name in &class_info.interfaces {
            if interface_constant_reachable(checker, interface_name, member, &mut HashSet::new()) {
                return true;
            }
        }
        current = class_info.parent.clone();
    }
    false
}

/// Walks an interface's own constants and its parent interfaces for `member`.
/// `visited` guards against cyclic interface graphs.
fn interface_constant_reachable(
    checker: &Checker,
    interface_name: &str,
    member: &str,
    visited: &mut HashSet<String>,
) -> bool {
    if !visited.insert(interface_name.to_string()) {
        return false;
    }
    let Some(interface_info) = checker.interfaces.get(interface_name) else {
        return false;
    };
    if interface_info.constants.contains_key(member) {
        return true;
    }
    interface_info
        .parents
        .iter()
        .any(|parent| interface_constant_reachable(checker, parent, member, visited))
}
