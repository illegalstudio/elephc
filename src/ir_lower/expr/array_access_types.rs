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

/// What lowering asks of the whole class table, tagged with the table it was computed from
/// (address and size).
#[derive(Default)]
struct ClassTableFacts {
    address: usize,
    len: usize,
    /// Interface (lowercased, no leading `\`) to every class implementing it, built in one pass
    /// over the class table on the first question (`interface_class_index`).
    interface_classes: Option<crate::fast_hash::FastMap<String, ImplementorSet>>,
    /// The same sets, keyed by the interface as spelled by the question.
    implementors: crate::fast_hash::FastMap<String, Option<ImplementorSet>>,
    /// Whether a class the program declared itself implements `ArrayAccess`.
    declares_array_access: Option<bool>,
    /// The program's classes with internal operator handlers, sorted by name.
    operator_classes: Option<std::rc::Rc<Vec<String>>>,
    /// Method key -> what the program's declarations of it take; see `boxed_method_shape`.
    boxed_method_shapes: crate::fast_hash::FastMap<String, BoxedMethodShape>,
}

/// What every class's declaration of one method name takes, as a call on a receiver of unknown
/// class must plan for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ir_lower) struct BoxedMethodShape {
    /// Some declaration takes a parameter by reference.
    pub(in crate::ir_lower) by_ref_param: bool,
    /// Some declaration is variadic in source (not the hidden `func_get_args()` collector).
    pub(in crate::ir_lower) source_variadic: bool,
}

/// What the program's declarations of method `key` take, over every class.
///
/// Asked by every call on a boxed receiver; each walked the whole class table again, about 340
/// samples of a sampled one-core Symfony demo build.
pub(in crate::ir_lower) fn boxed_method_shape(ctx: &LoweringContext<'_, '_>, key: &str) -> BoxedMethodShape {
    with_class_table_facts(ctx, |facts| {
        if let Some(shape) = facts.boxed_method_shapes.get(key) {
            return *shape;
        }
        #[cfg(test)]
        CLASS_TABLE_WALKS.with(|walks| walks.set(walks.get() + 1));
        let mut shape = BoxedMethodShape::default();
        for class_info in ctx.classes.values() {
            let Some(sig) = class_info.methods.get(key) else {
                continue;
            };
            shape.by_ref_param |= sig.ref_params.iter().any(|by_ref| *by_ref);
            shape.source_variadic |=
                sig.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(sig);
        }
        facts.boxed_method_shapes.insert(key.to_string(), shape);
        shape
    })
}

/// The program's classes with internal operator handlers (`GMP`, `BcMath\Number`, ...), sorted.
///
/// Asked by every binary, comparison and cast operation on a `Mixed` or object operand. Each
/// asking filtered and sorted the whole class table again, and the per-body answer to whether
/// there is any such class scanned it once per body: about 400 samples of a sampled one-core
/// Symfony demo build.
pub(in crate::ir_lower) fn program_operator_classes(ctx: &LoweringContext<'_, '_>) -> std::rc::Rc<Vec<String>> {
    with_class_table_facts(ctx, |facts| {
        facts
            .operator_classes
            .get_or_insert_with(|| {
                #[cfg(test)]
                CLASS_TABLE_WALKS.with(|walks| walks.set(walks.get() + 1));
                let mut names: Vec<String> = ctx
                    .classes
                    .keys()
                    .filter(|name| elephc_builtin_contract::is_operator_class_name(name))
                    .cloned()
                    .collect();
                names.sort();
                std::rc::Rc::new(names)
            })
            .clone()
    })
}

thread_local! {
    static CLASS_TABLE_FACTS: std::cell::RefCell<ClassTableFacts> =
        std::cell::RefCell::new(ClassTableFacts::default());
}

#[cfg(test)]
thread_local! {
    /// Whole-table walks answering [`class_implements_interface_for_ir`] and
    /// [`program_declares_array_access`].
    pub(crate) static CLASS_TABLE_WALKS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// Forgets the facts; a later program's class table could reuse an address and a size.
pub(crate) fn reset_class_table_facts() {
    CLASS_TABLE_FACTS.with(|facts| *facts.borrow_mut() = ClassTableFacts::default());
}

/// Runs `read` on the facts of this context's class table, which is fixed while a program lowers.
fn with_class_table_facts<T>(
    ctx: &LoweringContext<'_, '_>,
    read: impl FnOnce(&mut ClassTableFacts) -> T,
) -> T {
    let (address, len) = (ctx.classes as *const _ as usize, ctx.classes.len());
    CLASS_TABLE_FACTS.with(|facts| {
        let mut facts = facts.borrow_mut();
        if (facts.address, facts.len) != (address, len) {
            *facts = ClassTableFacts { address, len, ..ClassTableFacts::default() };
        }
        read(&mut facts)
    })
}

/// Returns whether a lowered class implements an interface, following parents.
///
/// Whole-program questions ask this of every class per call site, each answer walking parents
/// and interface trees, so the classes of one interface are collected once.
pub(in crate::ir_lower) fn class_implements_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    with_class_table_facts(ctx, |facts| {
        if !facts.implementors.contains_key(interface_name) {
            let index = facts.interface_classes.get_or_insert_with(|| interface_class_index(ctx));
            let key = interface_name.trim_start_matches('\\').to_ascii_lowercase();
            let classes = index.get(&key).cloned();
            facts.implementors.insert(interface_name.to_string(), classes);
        }
        let implements = facts.implementors[interface_name]
            .as_ref()
            .is_some_and(|classes| classes.contains(class_name.trim_start_matches('\\')));
        // Every lowering test checks the index against the walk it replaced.
        #[cfg(test)]
        if ctx.classes.contains_key(class_name.trim_start_matches('\\')) {
            assert_eq!(
                implements,
                class_walk_implements_interface(ctx, class_name, interface_name),
                "{class_name} implements {interface_name}"
            );
        }
        implements
    })
}

/// The classes implementing one interface.
type ImplementorSet = std::rc::Rc<crate::fast_hash::FastSet<String>>;

/// Every interface (lowercased, no leading `\`) to the classes implementing it, through parents
/// and interface parents, as [`class_walk_implements_interface`] answers it for one pair.
///
/// One pass over the class table: each interface asked used to walk every class's parent chain
/// and interface tree again, about 780 samples of a sampled one-core Symfony demo build.
fn interface_class_index(ctx: &LoweringContext<'_, '_>) -> crate::fast_hash::FastMap<String, ImplementorSet> {
    #[cfg(test)]
    CLASS_TABLE_WALKS.with(|walks| walks.set(walks.get() + 1));
    let mut closures: crate::fast_hash::FastMap<String, std::rc::Rc<Vec<String>>> = Default::default();
    let mut index: crate::fast_hash::FastMap<String, crate::fast_hash::FastSet<String>> = Default::default();
    for class in ctx.classes.keys() {
        let mut implemented: crate::fast_hash::FastSet<String> = Default::default();
        let mut current = Some(class.trim_start_matches('\\'));
        while let Some(candidate) = current {
            let Some(info) = ctx.classes.get(candidate) else {
                break;
            };
            for interface in &info.interfaces {
                implemented.extend(interface_closure(ctx, interface.trim_start_matches('\\'), &mut closures).iter().cloned());
            }
            current = info.parent.as_deref();
        }
        for interface in implemented {
            index.entry(interface).or_default().insert(class.clone());
        }
    }
    index.into_iter().map(|(interface, classes)| (interface, std::rc::Rc::new(classes))).collect()
}

/// An interface and every interface it extends, lowercased, as
/// [`interface_extends_interface_for_ir`] follows them.
fn interface_closure(
    ctx: &LoweringContext<'_, '_>,
    interface: &str,
    closures: &mut crate::fast_hash::FastMap<String, std::rc::Rc<Vec<String>>>,
) -> std::rc::Rc<Vec<String>> {
    if let Some(known) = closures.get(interface) {
        return known.clone();
    }
    // A cycle reads its own placeholder rather than recursing forever.
    closures.insert(interface.to_string(), std::rc::Rc::new(Vec::new()));
    let mut names = vec![interface.to_ascii_lowercase()];
    if let Some(info) = ctx.interfaces.get(interface) {
        for parent in &info.parents {
            for name in interface_closure(ctx, parent.trim_start_matches('\\'), closures).iter() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
    }
    let names = std::rc::Rc::new(names);
    closures.insert(interface.to_string(), names.clone());
    names
}

/// Whether a class the program declared itself implements `ArrayAccess`: the built-in containers
/// and the compiler's own `__Elephc*` subclasses of them do not count.
pub(super) fn program_declares_array_access(ctx: &LoweringContext<'_, '_>) -> bool {
    if let Some(known) = with_class_table_facts(ctx, |facts| facts.declares_array_access) {
        return known;
    }
    #[cfg(test)]
    CLASS_TABLE_WALKS.with(|walks| walks.set(walks.get() + 1));
    let declares = ctx.classes.keys().any(|class| {
        elephc_builtin_contract::lookup_class(class).is_none()
            && !class.trim_start_matches('\\').to_ascii_lowercase().starts_with("__elephc")
            && class_implements_interface_for_ir(ctx, class, "ArrayAccess")
    });
    with_class_table_facts(ctx, |facts| facts.declares_array_access = Some(declares));
    declares
}

/// Walks one class's parents and interface trees: the reference `interface_class_index` is
/// checked against under test.
#[cfg(test)]
fn class_walk_implements_interface(
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
pub(crate) fn interface_extends_interface_for_ir(
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
    let mut visited = crate::fast_hash::FastSet::default();
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
