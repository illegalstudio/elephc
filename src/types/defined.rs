//! Purpose:
//! Resolves whether a `defined()` string names a declared class-like constant
//! or enum case using checker/EIR class, interface, and enum metadata.
//!
//! Called from:
//! - `crate::ir_lower::expr::constants::lower_static_defined_call`
//! - `crate::codegen::lower_inst::builtins::scalar_metadata::lower_defined`
//!
//! Key details:
//! - String names are global: a leading `\` is stripped, and namespace/`use`
//!   aliases are not applied.
//! - Class-like names are case-insensitive; constant and enum-case names are
//!   case-sensitive. Missing class-like types or members yield `false`.
//! - Existence is scope-aware: public is visible, private only from the
//!   declaring class, and protected from the declaring class's inheritance family.
//! - `self::` and `parent::` use the lexical class scope. `static::` is detected
//!   separately and lowered through runtime called-class dispatch.
//! - Relative receivers without their required class or parent scope raise
//!   catchable PHP `Error`s instead of becoming ordinary misses.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, EnumInfo, InterfaceInfo};

/// Returns true when `name` is a `Class::CONST` form visible from `current_class`.
///
/// `current_class` is the lexical class scope (`None` in global functions).
/// Inheritance and implemented-interface constants are included. EIR metadata
/// composes trait constants into using classes before this lookup. `static::`
/// is handled by the late-static lowering path before this helper is called.
pub(crate) fn class_like_constant_is_defined(
    classes: &HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    interfaces: &HashMap<String, InterfaceInfo>,
    enums: &HashMap<String, EnumInfo>,
    name: &str,
    current_class: Option<&str>,
) -> bool {
    let Some((class_name, member_name)) = split_class_const_name(name) else {
        return false;
    };
    let Some(class_name) = resolve_defined_receiver(classes, class_name, current_class) else {
        return false;
    };
    if enum_case_is_defined(enums, class_name, member_name) {
        return true;
    }
    if class_constant_is_defined(classes, interfaces, class_name, member_name, current_class) {
        return true;
    }
    interface_constant_is_defined(interfaces, class_name, member_name)
}

/// Returns PHP's `Error` message for a relative `defined()` receiver used in an invalid scope.
///
/// `self::` and `static::` require a lexical class. `parent::` additionally
/// requires that class to have a parent. Valid relative receivers and ordinary
/// class-like names return `None`.
pub(crate) fn class_like_constant_scope_error(
    classes: &HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    name: &str,
    current_class: Option<&str>,
) -> Option<String> {
    let (class_name, _) = split_class_const_name(name)?;
    let receiver = php_symbol_key(class_name);
    match receiver.as_str() {
        "self" | "static" if current_class.is_none() => Some(format!(
            "Cannot access \"{}\" when no class scope is active",
            receiver
        )),
        "parent" => {
            let Some(current) = current_class else {
                return Some(
                    "Cannot access \"parent\" when no class scope is active".to_string(),
                );
            };
            let has_parent = lookup_ci(classes, current)
                .and_then(|(_, info)| info.parent.as_ref())
                .is_some();
            (!has_parent).then(|| {
                "Cannot access \"parent\" when current class scope has no parent".to_string()
            })
        }
        _ => None,
    }
}

/// Returns the case-sensitive member from a `defined('static::MEMBER')` literal.
///
/// The relative receiver itself is case-insensitive. Other class-like forms and
/// malformed names return `None`.
pub(crate) fn defined_late_static_member(name: &str) -> Option<&str> {
    let (class_name, member_name) = split_class_const_name(name)?;
    (php_symbol_key(class_name) == "static").then_some(member_name)
}

/// Splits a `Class::CONST` string into `(class, member)` after stripping a leading `\`.
///
/// Uses the last `::` so namespaced class names such as `Foo\Bar::BAZ` stay intact.
fn split_class_const_name(name: &str) -> Option<(&str, &str)> {
    let trimmed = name.trim_start_matches('\\');
    let (class_name, member_name) = trimmed.rsplit_once("::")?;
    let class_name = class_name.trim_start_matches('\\');
    if class_name.is_empty() || member_name.is_empty() {
        return None;
    }
    Some((class_name, member_name))
}

/// Resolves `self::` / `parent::` against the lexical class; leaves `static::` for runtime lowering.
///
/// Relative receivers are case-insensitive, matching PHP. `static` is not
/// rewritten to the lexical class because its answer depends on the called class.
fn resolve_defined_receiver<'a>(
    classes: &'a HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    class_name: &'a str,
    current_class: Option<&str>,
) -> Option<&'a str> {
    match php_symbol_key(class_name).as_str() {
        "self" => {
            let current = current_class?;
            lookup_ci(classes, current).map(|(name, _)| name)
        }
        "parent" => {
            let current = current_class?;
            let (_, info) = lookup_ci(classes, current)?;
            let parent = info.parent.as_deref()?;
            lookup_ci(classes, parent).map(|(name, _)| name)
        }
        "static" => None,
        _ => Some(class_name),
    }
}

/// Returns true when `member_name` is a case-sensitive case of a matching enum.
fn enum_case_is_defined(
    enums: &HashMap<String, EnumInfo>,
    class_name: &str,
    member_name: &str,
) -> bool {
    let Some((_, enum_info)) = lookup_ci(enums, class_name) else {
        return false;
    };
    enum_info
        .cases
        .iter()
        .any(|case| case.name == member_name)
}

/// Returns true when `member_name` is a class constant visible from `current_class`.
///
/// Walks the receiver class toward parents and stops at the first inherited
/// declaration. Parent `private` constants are skipped because PHP does not
/// inherit them. An inaccessible first declaration does not fall through.
fn class_constant_is_defined(
    classes: &HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    interfaces: &HashMap<String, InterfaceInfo>,
    class_name: &str,
    member_name: &str,
    current_class: Option<&str>,
) -> bool {
    let Some((receiver_name, _)) = lookup_ci(classes, class_name) else {
        return false;
    };
    let mut current = Some(receiver_name);
    let mut seen = HashSet::new();
    while let Some(name) = current {
        if !seen.insert(php_symbol_key(name)) {
            break;
        }
        let Some(info) = classes.get(name) else {
            break;
        };
        if info.constants.contains_key(member_name) {
            let visibility = info
                .constant_visibilities
                .get(member_name)
                .cloned()
                .unwrap_or(Visibility::Public);
            if matches!(visibility, Visibility::Private)
                && php_symbol_key(name) != php_symbol_key(receiver_name)
            {
                current = parent_class_name(classes, info);
                continue;
            }
            return constant_is_visible(classes, current_class, name, &visibility);
        }
        current = parent_class_name(classes, info);
    }
    let Some(info) = classes.get(receiver_name) else {
        return false;
    };
    info.interfaces.iter().any(|interface_name| {
        interface_constant_is_defined(interfaces, interface_name, member_name)
    })
}

/// Returns whether `visibility` of a constant declared on `declaring_class` is
/// accessible from `current_class`.
fn constant_is_visible(
    classes: &HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    current_class: Option<&str>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => current_class.is_some_and(|scope| {
            php_symbol_key(scope.trim_start_matches('\\'))
                == php_symbol_key(declaring_class.trim_start_matches('\\'))
        }),
        Visibility::Protected => {
            let Some(scope) = current_class else {
                return false;
            };
            let Some((scope_name, _)) = lookup_ci(classes, scope) else {
                return false;
            };
            is_same_or_descendant(classes, scope_name, declaring_class)
                || is_same_or_descendant(classes, declaring_class, scope_name)
        }
    }
}

/// Returns the canonical parent class name, if the parent is in `classes`.
fn parent_class_name<'a>(
    classes: &'a HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    info: &ClassInfo,
) -> Option<&'a str> {
    lookup_ci(classes, info.parent.as_deref()?).map(|(name, _)| name)
}

/// Returns true when `class_name` is `ancestor` or a descendant of it.
fn is_same_or_descendant(
    classes: &HashMap<String, ClassInfo, impl std::hash::BuildHasher>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let ancestor_key = php_symbol_key(ancestor.trim_start_matches('\\'));
    let mut current = Some(class_name);
    let mut seen = HashSet::new();
    while let Some(name) = current {
        if !seen.insert(php_symbol_key(name)) {
            break;
        }
        if php_symbol_key(name.trim_start_matches('\\')) == ancestor_key {
            return true;
        }
        current = classes.get(name).and_then(|info| parent_class_name(classes, info));
    }
    false
}

/// Returns true when `member_name` is a constant on `interface_name` or a parent interface.
fn interface_constant_is_defined(
    interfaces: &HashMap<String, InterfaceInfo>,
    interface_name: &str,
    member_name: &str,
) -> bool {
    let mut visited = HashSet::new();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        let Some((resolved_name, info)) = lookup_ci(interfaces, &name) else {
            continue;
        };
        if !visited.insert(php_symbol_key(resolved_name)) {
            continue;
        }
        if info.constants.contains_key(member_name) {
            return true;
        }
        queue.extend(info.parents.iter().cloned());
    }
    false
}

/// Looks up a class-like schema by PHP's case-insensitive class-name rules.
fn lookup_ci<'a, T>(
    items: &'a HashMap<String, T, impl std::hash::BuildHasher>,
    name: &str,
) -> Option<(&'a str, &'a T)> {
    let trimmed = name.trim_start_matches('\\');
    if let Some((key, value)) = items.get_key_value(trimmed) {
        return Some((key.as_str(), value));
    }
    let key = php_symbol_key(trimmed);
    items.iter().find_map(|(candidate, value)| {
        (php_symbol_key(candidate.trim_start_matches('\\')) == key)
            .then_some((candidate.as_str(), value))
    })
}
