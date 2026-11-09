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

use crate::fast_hash::FastMap as HashMap;

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
    #[cfg(test)]
    SATISFIES_TESTS.with(|tests| tests.set(tests.get() + 1));
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

/// Whether `member`'s parent chain reaches the class `ancestor`, by name and without allocating.
/// Bounded by the class count, so a malformed inheritance cycle ends the walk.
fn descends_from(classes: &FastMap<String, ClassInfo>, member: &str, ancestor: &str) -> bool {
    #[cfg(test)]
    DESCENT_WALKS.with(|walks| walks.set(walks.get() + 1));
    let mut current = member.trim_start_matches('\\');
    for _ in 0..=classes.len() {
        let Some(parent) = classes.get(current).and_then(|info| info.parent.as_deref()) else {
            return false;
        };
        let parent = parent.trim_start_matches('\\');
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

#[cfg(test)]
thread_local! {
    /// Parent walks [`classes_satisfying_both`] made; read by the regression test below.
    static DESCENT_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Full `object_type_satisfies` tests made; read by the regression test below.
    static SATISFIES_TESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    // Every class is a candidate, and `object_type_satisfies` allocates its whole ancestry: two of
    // those per class per guard were 2,400 samples of a Symfony demo build's type check. When the
    // left side is a class, a candidate must descend from it, which a parent walk answers without
    // allocating; only those few pay the full test. (A class and an interface cannot share a name,
    // so for a class target the full test reduces to exactly this walk.)
    let left_is_interface = interfaces.contains_key(class_name);
    let mut found: Vec<String> = classes
        .keys()
        .filter(|name| {
            name.as_str() != class_name
                && if left_is_interface {
                    object_type_satisfies(classes, interfaces, name, class_name)
                } else {
                    descends_from(classes, name, class_name)
                }
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

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Regression tests for the cost of naming an `instanceof` intersection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The fixture goes through the real lexer, parser and checker, so the counters see every
    //!   guard the checker narrows, in every pass it makes.

    use super::{DESCENT_WALKS, SATISFIES_TESTS};

    /// Narrowing `Base & Marker` walks each class's parent chain, but runs the allocating full
    /// test only on the classes that descend from `Base`: 200 unrelated classes no longer each cost
    /// two ancestry walks per guard per pass.
    #[test]
    fn an_intersection_runs_the_full_test_only_on_descendants() {
        let mut source = String::from(
            "<?php\ninterface Marker { public function mark(): int; }\n\
             class Base { public int $n = 1; }\n\
             class Marked extends Base implements Marker { public function mark(): int { return 2; } }\n",
        );
        for index in 0..200 {
            source.push_str(&format!("class Unrelated{index} {{ public int $v = {index}; }}\n"));
        }
        source.push_str(
            "function probe(Base $b): int { if ($b instanceof Marker) { return $b->mark() + $b->n; } return 0; }\n\
             echo probe($argc > 1 ? new Marked() : new Base());\n",
        );
        let tokens = crate::lexer::tokenize(&source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        DESCENT_WALKS.with(|walks| walks.set(0));
        SATISFIES_TESTS.with(|tests| tests.set(0));
        crate::types::check(&program).expect("fixture must type check");
        let walks = DESCENT_WALKS.with(std::cell::Cell::get);
        let tests = SATISFIES_TESTS.with(std::cell::Cell::get);
        eprintln!("DESCENT_WALKS={walks} SATISFIES_TESTS={tests}");
        assert!(walks > 0, "the fixture must reach the intersection");
        assert!(tests * 10 < walks, "{tests} full tests for {walks} candidate classes");
    }
}

