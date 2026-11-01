//! `instanceof` narrowing as an INTERSECTION, answered from the closed world.
//!
//! `if ($x instanceof I)` does not replace what `$x` is, it adds to it. Replacing the type with
//! the target throws away the other half, and the halves carry different members: a class carries
//! the PROPERTIES, an interface carries the METHODS the guard was written to reach. Losing either
//! one produces a member lookup with nothing to answer from, which is how
//! `$this->context->getScheme()` ended up refused with `method call receiver for PHP type Int`.
//!
//! elephc has no intersection type, but it does not need one: it compiles a closed world, so it
//! can simply NAME the classes that satisfy both sides. That is what this module does, and it is
//! the one rule both the checker and the IR lowering consult -- they narrowed independently
//! before, and disagreeing about `$this` is what made the second read of a property fail where
//! the first had succeeded.

use std::collections::HashMap;

use crate::fast_hash::FastMap;
use crate::types::schema::{ClassInfo, InterfaceInfo};
use crate::types::PhpType;

/// How many concrete classes an intersection may name before narrowing gives up and keeps the
/// guard target. Every extra member costs a runtime class-id comparison at each member access, so
/// a guard matching a large part of the program is not worth encoding precisely -- and a guard
/// that broad proves little anyway.
pub(crate) const INTERSECTION_CANDIDATE_LIMIT: usize = 4;

/// Returns true when a value already typed `member` necessarily passes `instanceof target`:
/// the same name, a subclass, or an interface reached through the ancestor chain and the
/// interface-extension graph.
pub(crate) fn object_type_satisfies(
    classes: &FastMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    member: &str,
    target: &str,
) -> bool {
    let member = member.trim_start_matches('\\');
    let target = target.trim_start_matches('\\');
    if member == target {
        return true;
    }
    if interfaces.contains_key(member) {
        return interface_extends(interfaces, member, target);
    }
    ancestry(classes, member).into_iter().any(|ancestor| {
        classes.get(&ancestor).is_some_and(|info| {
            info.parent
                .as_deref()
                .is_some_and(|parent| parent.trim_start_matches('\\') == target)
                || info.interfaces.iter().any(|name| {
                    let name = name.trim_start_matches('\\');
                    name == target || interface_extends(interfaces, name, target)
                })
        })
    })
}

/// Returns true when `interface_name` is, or transitively extends, `ancestor`.
fn interface_extends(
    interfaces: &HashMap<String, InterfaceInfo>,
    interface_name: &str,
    ancestor: &str,
) -> bool {
    let mut stack = vec![interface_name.trim_start_matches('\\').to_string()];
    let mut seen: Vec<String> = Vec::new();
    while let Some(current) = stack.pop() {
        if current == ancestor {
            return true;
        }
        if seen.contains(&current) {
            continue;
        }
        let Some(info) = interfaces.get(&current) else {
            seen.push(current);
            continue;
        };
        for parent in &info.parents {
            stack.push(parent.trim_start_matches('\\').to_string());
        }
        seen.push(current);
    }
    false
}

/// `class_name` followed by every ancestor class, nearest first. A malformed inheritance cycle
/// terminates the walk instead of hanging the compiler.
fn ancestry(classes: &FastMap<String, ClassInfo>, class_name: &str) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if chain.contains(&name) {
            break;
        }
        current = classes
            .get(&name)
            .and_then(|info| info.parent.clone())
            .map(|parent| parent.trim_start_matches('\\').to_string());
        chain.push(name);
    }
    chain
}

/// The closed world's answer to `class_name & interface_name`: every declared class that descends
/// from `class_name` and also satisfies `interface_name`, sorted so the build stays deterministic.
pub(crate) fn classes_satisfying_both(
    classes: &FastMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    class_name: &str,
    interface_name: &str,
) -> Vec<String> {
    let class_name = class_name.trim_start_matches('\\');
    let mut found: Vec<String> = classes
        .keys()
        .filter(|name| {
            name.as_str() != class_name
                && object_type_satisfies(classes, interfaces, name, class_name)
                && object_type_satisfies(classes, interfaces, name, interface_name)
        })
        .cloned()
        .collect();
    found.sort();
    found
}

/// Narrows an object-typed value to what `instanceof target` proves about it, returning `None`
/// when the guard target alone is the right answer (which is what every caller did before this
/// module existed, and stays the fallback).
///
/// Three cases, in order:
///
/// 1. `current` already satisfies the target -- the guard proves nothing new, and the CLASS is
///    strictly more informative than the interface, so it survives.
/// 2. The target satisfies `current` (a more specific interface, e.g. Symfony's
///    `ContainerInterface` guarding a value typed as PSR's) -- the target is the narrowing.
/// 3. Neither is-a the other. `CompiledUrlMatcher` guarded by `RedirectableUrlMatcherInterface`
///    is this case: the guard exists precisely because a SUBCLASS implements the interface. The
///    value is necessarily one of those subclasses, so name them; each carries both halves.
pub(crate) fn narrow_object_to_instanceof(
    classes: &FastMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    current: &str,
    target: &str,
) -> Option<PhpType> {
    let current = current.trim_start_matches('\\');
    let target = target.trim_start_matches('\\');
    if current.is_empty() || target.is_empty() || current == target {
        return None;
    }
    if !classes.contains_key(current) && !interfaces.contains_key(current) {
        return None;
    }
    if object_type_satisfies(classes, interfaces, current, target) {
        return Some(PhpType::Object(current.to_string()));
    }
    if object_type_satisfies(classes, interfaces, target, current) {
        return None;
    }
    let candidates = classes_satisfying_both(classes, interfaces, current, target);
    if candidates.is_empty() || candidates.len() > INTERSECTION_CANDIDATE_LIMIT {
        return None;
    }
    if let [only] = candidates.as_slice() {
        return Some(PhpType::Object(only.clone()));
    }
    Some(PhpType::Union(
        candidates.into_iter().map(PhpType::Object).collect(),
    ))
}

