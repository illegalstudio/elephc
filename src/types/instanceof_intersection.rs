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
//!
//! The naming is only sound while the class table is the whole world. A program that loads code at
//! run time (an opaque `eval()`, a runtime include or autoloader, outside a closed-world build) can
//! declare another class that satisfies both sides, so there the guard target alone stands, as it
//! did before this module: [`set_open_world`] records that once per compilation for both callers.

use crate::fast_hash::FastMap as HashMap;

use crate::fast_hash::FastMap;
use crate::types::schema::{ClassInfo, InterfaceInfo};
use crate::types::PhpType;

/// How many concrete classes an intersection may name before narrowing gives up and keeps the
/// guard target. Every extra member costs a runtime class-id comparison at each member access, so
/// a guard matching a large part of the program is not worth encoding precisely -- and a guard
/// that broad proves little anyway.
pub(crate) const INTERSECTION_CANDIDATE_LIMIT: usize = 4;

thread_local! {
    /// Whether the compilation running on this thread can meet classes its table does not hold.
    static OPEN_WORLD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Records, before type checking, whether classes can appear at run time that the class table
/// does not hold; the checker and the IR lowering of the same compilation then agree on it.
pub(crate) fn set_open_world(open: bool) {
    OPEN_WORLD.with(|cell| cell.set(open));
}

/// Whether the compilation running on this thread can meet classes its table does not hold.
fn open_world() -> bool {
    OPEN_WORLD.with(std::cell::Cell::get)
}

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
    // `member` and each ancestor class, nearest first, borrowed from the table: this runs for
    // every class of the program on each interface-sided guard, so it must not allocate. The walk
    // is bounded by the class count, so a malformed inheritance cycle ends it.
    let mut current = member;
    for _ in 0..=classes.len() {
        let Some(info) = classes.get(current) else {
            return false;
        };
        if info.parent.as_deref().is_some_and(|parent| parent.trim_start_matches('\\') == target)
            || info.interfaces.iter().any(|name| {
                let name = name.trim_start_matches('\\');
                name == target || interface_extends(interfaces, name, target)
            })
        {
            return true;
        }
        let Some(parent) = info.parent.as_deref() else {
            return false;
        };
        current = parent.trim_start_matches('\\');
    }
    false
}

/// Returns true when `interface_name` is, or transitively extends, `ancestor`.
fn interface_extends<'a>(
    interfaces: &'a HashMap<String, InterfaceInfo>,
    interface_name: &'a str,
    ancestor: &str,
) -> bool {
    let interface_name = interface_name.trim_start_matches('\\');
    if interface_name == ancestor {
        return true;
    }
    let Some(info) = interfaces.get(interface_name) else {
        return false;
    };
    if info.parents.is_empty() {
        return false;
    }
    #[cfg(test)]
    INTERFACE_WALKS.with(|walks| walks.set(walks.get() + 1));
    let mut stack: Vec<&'a str> = vec![interface_name];
    let mut seen: Vec<&'a str> = Vec::new();
    while let Some(current) = stack.pop() {
        if current == ancestor {
            return true;
        }
        if seen.contains(&current) {
            continue;
        }
        seen.push(current);
        let Some(info) = interfaces.get(current) else {
            continue;
        };
        stack.extend(info.parents.iter().map(|parent| parent.trim_start_matches('\\')));
    }
    false
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
    /// Interface-extension walks that needed a stack; read by the regression test below.
    static INTERFACE_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    // Naming the classes that satisfy both halves needs every class: a class declared at run time
    // (`eval`, a runtime include) can satisfy them too, and typing it as a compiled one read its
    // properties from the wrong layout.
    if open_world() {
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

    use super::{DESCENT_WALKS, INTERFACE_WALKS, SATISFIES_TESTS};

    /// An interface-sided guard tests every class of the program against both interfaces. A class
    /// implementing an interface that extends nothing answers without walking the extension graph:
    /// 200 such classes cost no walk, where each test allocated a stack and the class's ancestry.
    #[test]
    fn an_interface_guard_walks_only_interfaces_that_extend_others() {
        let mut source = String::from(
            "<?php\ninterface Marker { public function mark(): int; }\n\
             interface Named { public function name(): string; }\n\
             class Both implements Marker, Named {\n\
                 public function mark(): int { return 1; }\n\
                 public function name(): string { return 'b'; }\n\
             }\n",
        );
        for index in 0..200 {
            source.push_str(&format!(
                "interface Own{index} {{}}\nclass Plain{index} implements Own{index}, Marker {{ public function mark(): int {{ return {index}; }} }}\n"
            ));
        }
        source.push_str(
            "function probe(Marker $m): string { if ($m instanceof Named) { return $m->name(); } return 'none'; }\n\
             echo probe($argc > 1 ? new Both() : new Plain0());\n",
        );
        let tokens = crate::lexer::tokenize(&source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        SATISFIES_TESTS.with(|tests| tests.set(0));
        INTERFACE_WALKS.with(|walks| walks.set(0));
        crate::types::check(&program).expect("fixture must type check");
        let tests = SATISFIES_TESTS.with(std::cell::Cell::get);
        let walks = INTERFACE_WALKS.with(std::cell::Cell::get);
        assert!(tests > 200, "the fixture must reach the interface-sided intersection: {tests}");
        assert!(walks <= 2, "{walks} interface walks for {tests} tests");
    }

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

