//! Purpose:
//! ArrayAccess interface and method-return metadata queries.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns true when every possible object arm satisfies PHP's `ArrayAccess` interface.
pub(crate) fn type_satisfies_array_access_for_ir(
    ctx: &LoweringContext<'_, '_>,
    ty: &PhpType,
) -> bool {
    match ty {
        PhpType::Object(class_name) => {
            object_name_satisfies_interface_for_ir(ctx, class_name, "ArrayAccess")
        }
        PhpType::Union(members) => {
            let mut saw_object = false;
            for member in members {
                match member {
                    PhpType::Void | PhpType::Never => {}
                    other if type_satisfies_array_access_for_ir(ctx, other) => {
                        saw_object = true;
                    }
                    _ => return false,
                }
            }
            saw_object
        }
        _ => false,
    }
}

/// Returns true when a class or interface name satisfies the requested interface.
pub(super) fn object_name_satisfies_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    if php_symbol_key(normalized) == php_symbol_key(interface_name.trim_start_matches('\\')) {
        return true;
    }
    if ctx.interfaces.contains_key(normalized) {
        return interface_extends_interface_for_ir(ctx, normalized, interface_name);
    }
    class_implements_interface_for_ir(ctx, normalized, interface_name)
}

/// Returns whether a lowered class implements an interface, following parents.
pub(in crate::ir_lower) fn class_implements_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    // `php_symbol_key` is `to_ascii_lowercase`, so an equality between two of its results is
    // `eq_ignore_ascii_case` with a `String` allocated per candidate. This predicate runs per
    // parent hop per interface per call, and the recursion below multiplied that again.
    let wanted = interface_name.trim_start_matches('\\');
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        let Some(info) = ctx.classes.get(candidate) else {
            return false;
        };
        if info
            .interfaces
            .iter()
            .any(|interface| {
                let interface = interface.trim_start_matches('\\');
                interface.eq_ignore_ascii_case(wanted)
                    || interface_extends_interface_for_ir(ctx, interface, interface_name)
            })
        {
            return true;
        }
        current = info.parent.as_deref();
    }
    false
}

/// Returns true when an interface extends the requested ancestor interface.
pub(super) fn interface_extends_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    ancestor_name: &str,
) -> bool {
    let ancestor = ancestor_name.trim_start_matches('\\');
    if interface_name.trim_start_matches('\\').eq_ignore_ascii_case(ancestor) {
        return true;
    }
    let Some(info) = ctx.interfaces.get(interface_name.trim_start_matches('\\')) else {
        return false;
    };
    info.parents.iter().any(|parent| {
        let parent = parent.trim_start_matches('\\');
        parent.eq_ignore_ascii_case(ancestor)
            || interface_extends_interface_for_ir(ctx, parent, ancestor_name)
    })
}

/// Returns a method return type from class metadata, following parent classes.
pub(super) fn class_method_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<PhpType> {
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        let info = ctx.classes.get(candidate)?;
        if let Some(sig) = info.methods.get(method_key) {
            return Some(sig.return_type.clone());
        }
        current = info.parent.as_deref();
    }
    None
}

/// Returns a method return type from interface metadata, following interface parents.
pub(super) fn interface_method_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    method_key: &str,
) -> Option<PhpType> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.trim_start_matches('\\').to_string()];
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(info) = ctx.interfaces.get(&name) else {
            continue;
        };
        if let Some(sig) = info.methods.get(method_key) {
            return Some(sig.return_type.clone());
        }
        queue.extend(info.parents.iter().cloned());
    }
    None
}
