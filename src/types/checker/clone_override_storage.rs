//! Purpose:
//! Gives untyped properties reached by runtime-shaped mutation the boxed storage their PHP
//! semantics require, currently clone overrides and property `unset()`.
//!
//! Called from:
//! - `crate::builtins::callables::clone`'s check hook, which records the destination classes.
//! - The compiler-resident `unset` checker, which records reachable fixed property slots.
//! - `Checker::infer_closure_call_type`, for a runtime string or boxed callable whose callee may
//!   be `clone` and whose destination is therefore unknowable.
//! - `crate::types::checker::check_types_with_options`, which applies the widening once every
//!   body has been checked.
//!
//! Key details:
//! - A PHP property WITHOUT a declared type is `mixed`: it accepts an int, a string, `null`, an
//!   array and an object with no coercion and no type error. This compiler instead infers such a
//!   slot from its default (`public $u = 0;` becomes `int`, `public $u;` becomes `null`), which
//!   is sound only because `refine_object_property_type` re-widens the slot the moment an
//!   ordinary `$o->u = $value;` assigns something else. A `clone()` override is exactly such an
//!   assignment, but it is SYNTHESIZED after checking, so nothing ever widened the slot for it:
//!   an inferred `int` slot silently coerced `"hello"` to `int(0)`, and an inferred `null` slot
//!   failed the whole build with `prop_set assigning PHP type Mixed to U::$u with PHP type Void`.
//!   Property `unset()` has the same representation requirement: a later read returns null and a
//!   later assignment can store any value, while the fixed slot also needs an explicit absent
//!   marker for `isset()` and object-property enumeration.
//! - The widening is selected from DESTINATION classes only, never from every class in the
//!   program. The `clone` check hook knows the first argument's inferred type, so
//!   `clone($item, [...])` selects the slots of `Item` and its subclasses and leaves every
//!   unrelated class alone. Only a call whose object type is runtime-shaped selects every user
//!   class.
//! - What is selected is a physical SLOT, `(declaring class, slot index)`, not a class. One slot is
//!   a single piece of storage that several `ClassInfo` copies describe, so it is stamped in the
//!   class that declared it and in every class that inherits it. Stamping the destination's copy
//!   alone left the declaring class reading the same bytes with its older inferred type, which
//!   turned a boxed value back into a raw pointer and crashed on an object-valued slot. Two
//!   siblings' OWN slots that merely share an index are different storage and never propagate.
//! - Declared slots keep their declared type. PHP applies weak-mode property typing to them, and
//!   `mixed_property_type_guard` already implements it for runtime-shaped values.
//! - Packed/extern classes are left alone: their slot holds a packed field rather than a value, so
//!   a Mixed stamp would describe the wrong storage.
//! - A reference slot IS widened when its property is undeclared. `property_reference_slots`
//!   records that the slot physically holds a shared cell; `properties[slot].1` records that
//!   cell's PAYLOAD, and an undeclared property's payload is `mixed` with or without an alias.
//! - The same destination set also reserves the per-instance property HASH an override key whose
//!   name matches no declared slot needs. It is a storage capability only: creating the property
//!   still emits php 8.5's `Creation of dynamic property C::$n is deprecated`, and the checker
//!   never consults `has_property_hash_storage()`, so `$ordinary->undeclared = 1` keeps its
//!   compile-time refusal.

use std::collections::{BTreeMap, BTreeSet};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{ClassInfo, PhpType, TypeEnv};

use super::Checker;

/// The clone-with destination classes one program can reach.
#[derive(Debug, Default, Clone)]
pub struct CloneOverrideDestinations {
    /// A site passes an object whose class the checker could not name.
    pub any_class: bool,
    /// Statically known destination class names, before subclass expansion.
    pub classes: BTreeSet<String>,
}

/// Untyped fixed property slots that a reachable `unset()` can remove.
///
/// The property name is absent for a runtime name, and the class name is absent for a boxed
/// receiver. Keeping both dimensions lets the final widening touch only slots the operation can
/// actually reach while still covering runtime subclasses.
#[derive(Debug, Default, Clone)]
pub struct PropertyUnsetDestinations {
    sites: BTreeSet<(Option<String>, Option<String>)>,
}

/// Records the fixed slots one property `unset()` may reach.
pub(in crate::types::checker) fn record_property_unset_destination(
    checker: &mut Checker,
    object_ty: &PhpType,
    property: Option<&str>,
) {
    let property = property.map(str::to_string);
    match object_ty.codegen_repr() {
        PhpType::Object(class_name) if !class_name.is_empty() => {
            checker
                .property_unset_destinations
                .sites
                .insert((Some(class_name), property));
        }
        // The backend represents every union through the Mixed runtime-class ladder, whose
        // candidate table currently covers every user class. Match that reachability here so an
        // unrelated untyped candidate cannot retain a refined shape and reject the whole ladder.
        PhpType::Union(_) | PhpType::Mixed => {
            checker
                .property_unset_destinations
                .sites
                .insert((None, property));
        }
        _ => {}
    }
}

impl CloneOverrideDestinations {
    /// Records one two-argument `clone()` site from its inferred first-argument type.
    pub fn record(&mut self, object_ty: &PhpType) {
        match object_ty.codegen_repr() {
            PhpType::Object(class) if !class.is_empty() => {
                self.classes.insert(class);
            }
            _ => self.any_class = true,
        }
    }

    /// Returns whether this program reaches a two-argument `clone()` at all.
    fn is_empty(&self) -> bool {
        !self.any_class && self.classes.is_empty()
    }
}

/// Records the widening destination a first-class `clone()` callable call reaches.
///
/// `clone(...)` is checked through its callable SIGNATURE, not through the builtin check hook, so
/// nothing recorded the destination for `$f = clone(...); $f($object, [...])`. An untyped
/// `public $u = 0;` slot then coerced an override's string back to `int(0)`, and an untyped
/// `public $u;` slot failed the build with `prop_set assigning PHP type Mixed to U::$u with PHP
/// type Void`, exactly the two failures `widen_clone_override_property_storage` exists to prevent.
///
/// Only a site that can actually carry the optional `withProperties` argument records anything,
/// and a statically named first argument records only its own class, so unrelated classes are
/// left alone.
pub(in crate::types::checker) fn record_callable_clone_override_destination(
    checker: &mut Checker,
    args: &[Expr],
    env: &TypeEnv,
) -> Result<(), CompileError> {
    if !clone_call_may_carry_overrides(args) {
        return Ok(());
    }
    match args.first() {
        // An unpack or a named argument does not pin which expression is the object, so the
        // destination really is unknowable and every user class has to be widened.
        Some(object)
            if !matches!(object.kind, ExprKind::Spread(_) | ExprKind::NamedArg { .. }) =>
        {
            let object_ty = checker.infer_type(object, env)?;
            checker.clone_override_destinations.record(&object_ty);
        }
        _ => checker.clone_override_destinations.any_class = true,
    }
    Ok(())
}

/// Records the widening a runtime string or boxed callable invocation can reach.
///
/// `$f = 'clone'; $f($object, [...])` resolves its callee at runtime, so neither the builtin check
/// hook nor the callable-signature path ever sees a `clone` call. The callable set behind such a
/// variable is not tracked as an exact string, so it MAY be `clone`, and the only sound record is
/// the runtime-shaped one: every user class this program declares is widened, exactly like a
/// `clone($object, [...])` whose first argument type is boxed.
///
/// The widening it requests is PHP-correct on its own terms: it only re-stamps property slots that
/// carry no declared type, and a PHP property without a declared type IS `mixed`. A declared slot
/// is left alone by `slot_is_widenable`, and a packed class, an enum and a checker-injected class
/// by `owns_physical_layout`.
///
/// A one-argument invocation can never write a property, so it records nothing.
pub(in crate::types::checker) fn record_runtime_callable_clone_override_destination(
    checker: &mut Checker,
    args: &[Expr],
) {
    if !clone_call_may_carry_overrides(args) {
        return;
    }
    checker.clone_override_destinations.any_class = true;
}

/// Returns whether a `clone()` call site can carry the optional `withProperties` overrides.
///
/// A one-argument `clone($object)` never writes a property, so it must not widen anything.
fn clone_call_may_carry_overrides(args: &[Expr]) -> bool {
    args.len() >= 2
        || args.iter().any(|arg| match &arg.kind {
            // An unpack carries an unknown number of values, so the overrides may be inside it.
            ExprKind::Spread(_) => true,
            ExprKind::NamedArg { name, .. } => name == "withProperties",
            _ => false,
        })
}

/// Widens every undeclared property slot a `clone()` override can write to `mixed`.
///
/// Runs after body checking, next to `apply_reference_property_promotions`, because both change
/// property STORAGE metadata that EIR lowering re-reads from `Checker::classes` rather than from
/// a per-span record.
pub(super) fn widen_clone_override_property_storage(checker: &mut Checker) {
    let destinations = std::mem::take(&mut checker.clone_override_destinations);
    if destinations.is_empty() {
        return;
    }
    let sel = destination_classes(checker, &destinations);
    for class_name in &sel {
        reserve_property_hash_storage(checker, class_name);
    }
    // The hash reservation stays on the destinations, but the WIDENING has to follow the physical
    // SLOT. One slot is a single piece of storage seen through several `ClassInfo` copies, so
    // restamping the destination's copy alone leaves the class that DECLARED it reading and writing
    // the same bytes with the type it inferred before: an inherited `private $n = 1;` became
    // `Mixed` on the child and stayed `Int` on the parent, so the parent's own accessor read the
    // child's boxed value back as a raw pointer and a closure-valued slot crashed the process.
    let views = layout_views(checker);
    for (owner, slot) in widened_slots(&views, &sel) {
        for target in slot_inheritors(&views, &owner, slot) {
            if let Some(info) = checker.classes.get_mut(&target) {
                info.properties[slot].1 = PhpType::Mixed;
            }
        }
    }
}

/// Widens every untyped fixed slot a reachable `unset()` can remove to boxed `Mixed` storage.
///
/// A slot removed by PHP subsequently reads as `null`, may be assigned any PHP value, and is not
/// reported by `isset()`. The boxed shape plus the slot's otherwise-unused high word can represent
/// all three states without changing unrelated refined properties.
pub(super) fn widen_property_unset_storage(checker: &mut Checker) {
    let destinations = std::mem::take(&mut checker.property_unset_destinations);
    if destinations.sites.is_empty() {
        return;
    }
    let views = layout_views(checker);
    let mut selected_slots = BTreeSet::new();
    for (class, property) in destinations.sites {
        let classes = match class {
            Some(class_name) => destination_classes(
                checker,
                &CloneOverrideDestinations {
                    any_class: false,
                    classes: BTreeSet::from([class_name]),
                },
            ),
            None => destination_classes(
                checker,
                &CloneOverrideDestinations {
                    any_class: true,
                    classes: BTreeSet::new(),
                },
            ),
        };
        for class_name in classes {
            let Some(info) = checker.classes.get(&class_name) else {
                continue;
            };
            let Some(view) = views.get(&class_name) else {
                continue;
            };
            if view.owns_layout {
                continue;
            }
            for (slot, (slot_name, _)) in info.properties.iter().enumerate() {
                if property.as_ref().is_some_and(|name| name != slot_name)
                    || info.visible_property_index(slot_name) != Some(slot)
                    || info.property_slot_is_reference(slot, slot_name)
                    || !view.widenable.get(slot).copied().unwrap_or(false)
                {
                    continue;
                }
                let owner = slot_declaring_class(&views, &class_name, slot);
                if views.get(&owner).is_some_and(|owner| owner.owns_layout) {
                    continue;
                }
                selected_slots.insert((owner, slot));
            }
        }
    }
    for (owner, slot) in selected_slots {
        for target in slot_inheritors(&views, &owner, slot) {
            let may_mark_removed = checker.classes.get(&target).is_some_and(|info| {
                info.properties.get(slot).is_some_and(|(name, _)| {
                    !info.property_slot_is_reference(slot, name)
                })
            });
            if !may_mark_removed {
                continue;
            }
            if let Some(info) = checker.classes.get_mut(&target) {
                info.properties[slot].1 = PhpType::Mixed;
            }
        }
    }
}

/// One class's layout facts the slot selection needs.
///
/// Lifted out of `ClassInfo` so the selection rule itself is a pure function over a small graph,
/// which is what the unit tests at the bottom of this file exercise directly.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LayoutView {
    /// Declared parent, which owns the FIRST entries of this class's physical layout.
    parent: Option<String>,
    /// Per physical slot index, whether the clone-override widening may restamp it here.
    widenable: Vec<bool>,
    /// Whether this class's layout is owned elsewhere and must never be restamped.
    owns_layout: bool,
}

/// Lifts the layout facts of every class the checker knows.
fn layout_views(checker: &Checker) -> BTreeMap<String, LayoutView> {
    checker
        .classes
        .iter()
        .map(|(class_name, info)| {
            let view = LayoutView {
                parent: info.parent.clone(),
                widenable: (0..info.properties.len())
                    .map(|slot| slot_is_widenable(info, slot))
                    .collect(),
                owns_layout: owns_physical_layout(checker, class_name),
            };
            (class_name.clone(), view)
        })
        .collect()
}

/// Selects the exact PHYSICAL slots a `clone()` override on these destinations can write.
///
/// Keyed by `(declaring class, slot index)` rather than by class on purpose. `ChildA`'s own slot 1
/// and `ChildB`'s own slot 1 are DIFFERENT storage that merely share an index, so widening one must
/// never reach the other; only a slot an ancestor really declared is shared. Resolving the owner by
/// index rather than by name matters too: a child that redeclares a parent's PRIVATE property gets
/// a fresh slot appended after the parent's and both carry the same name, so a name lookup would
/// report the child for the parent's slot.
fn widened_slots(
    views: &BTreeMap<String, LayoutView>,
    destinations: &[String],
) -> BTreeSet<(String, usize)> {
    let mut slots = BTreeSet::new();
    for class_name in destinations {
        let Some(view) = views.get(class_name) else {
            continue;
        };
        if view.owns_layout {
            continue;
        }
        for (slot, widenable) in view.widenable.iter().enumerate() {
            if !widenable {
                continue;
            }
            let owner = slot_declaring_class(views, class_name, slot);
            // A catalog builtin, a packed class and an enum own their layout together with the code
            // that reads it, so a user subclass must not restamp a slot it merely INHERITED from
            // one of them: the subclass is widened itself, its inherited storage is not.
            if views.get(&owner).is_some_and(|owner| owner.owns_layout) {
                continue;
            }
            slots.insert((owner, slot));
        }
    }
    slots
}

/// Returns the class that DECLARED one physical slot index of `class_name`.
///
/// `ClassBuildState::inherit_properties` pushes the parent's slots first and in order, so the
/// root-most ancestor that still HAS this index is the class the storage belongs to.
fn slot_declaring_class(
    views: &BTreeMap<String, LayoutView>,
    class_name: &str,
    slot: usize,
) -> String {
    let mut owner = class_name.to_string();
    let mut current = class_name.to_string();
    for _ in 0..=views.len() {
        let Some(parent) = views.get(&current).and_then(|view| view.parent.clone()) else {
            break;
        };
        match views.get(&parent) {
            Some(view) if slot < view.widenable.len() => owner = parent.clone(),
            _ => break,
        }
        current = parent;
    }
    owner
}

/// Returns the declaring class plus every class that INHERITS that one physical slot.
///
/// A class that redeclares the slot with a declared type is skipped rather than restamped, which
/// is the same rule `slot_is_widenable` applies to the destination itself.
fn slot_inheritors(
    views: &BTreeMap<String, LayoutView>,
    owner: &str,
    slot: usize,
) -> BTreeSet<String> {
    views
        .iter()
        .filter(|(class_name, view)| {
            !view.owns_layout
                && view.widenable.get(slot).copied().unwrap_or(false)
                && (class_name.as_str() == owner || inherits_from(views, class_name, owner))
        })
        .map(|(class_name, _)| class_name.clone())
        .collect()
}

/// Returns whether `child` reaches `ancestor` through the declared parent chain.
fn inherits_from(views: &BTreeMap<String, LayoutView>, child: &str, ancestor: &str) -> bool {
    let mut current = views.get(child).and_then(|view| view.parent.clone());
    for _ in 0..=views.len() {
        let Some(name) = current else {
            return false;
        };
        if name == ancestor {
            return true;
        }
        current = views.get(&name).and_then(|view| view.parent.clone());
    }
    false
}

/// Reserves the per-instance property hash one class needs for an UNKNOWN override key.
///
/// php 8.5 stores `clone($object, ["zz" => "x"])` on an ordinary class and deprecates the
/// creation; without a hash there is nowhere to put it, and the applicator could only refuse the
/// write. Reserving it here rather than on every object keeps the cost to the destination classes
/// this program's own `clone()` sites can actually reach: one trailing pointer per instance, and
/// the clone/free/GC traversal `#[\AllowDynamicProperties]` instances already pay.
///
/// The flag is a STORAGE capability, never PHP's permission. `allow_dynamic_properties` stays
/// exactly as the attribute left it, so `ClassInfo::dynamic_property_creation_is_deprecated()`
/// still separates the exempt classes from the deprecated ones, and `__set()` keeps its
/// precedence in `crate::ir_lower::clone_overrides::arms`.
fn reserve_property_hash_storage(checker: &mut Checker, class_name: &str) {
    // Same exclusions as `widen_class`: a checker-injected builtin, a packed class and an enum
    // all own their physical layout together with the code that reads it.
    //
    // The builtin catalog, NOT `declared_classes`, is the authority for the first of those. The
    // driver repopulates `declared_classes` from the class map AFTER builtin injection, so that
    // set contains the injected SPL, Reflection, DateTime and throwable classes too and excludes
    // nothing here. A user subclass of a builtin is absent from the catalog and stays eligible.
    if elephc_builtin_contract::lookup_class(class_name).is_some()
        || !checker.declared_classes.contains(class_name)
        || checker.packed_classes.contains_key(class_name)
        || checker.enums.contains_key(class_name)
        || super::builtin_stdclass::is_stdclass(class_name)
    {
        return;
    }
    let Some(info) = checker.classes.get_mut(class_name) else {
        return;
    };
    if info.has_property_hash_storage() {
        return;
    }
    info.clone_override_property_storage = true;
}

/// Expands the recorded destinations into the exact set of classes to widen.
fn destination_classes(checker: &Checker, destinations: &CloneOverrideDestinations) -> Vec<String> {
    let mut names = BTreeSet::new();
    for (candidate, _) in checker.classes.iter() {
        if destinations.any_class
            || destinations.classes.contains(candidate)
            || destinations
                .classes
                .iter()
                .any(|destination| checker.is_subclass_of(candidate, destination))
        {
            names.insert(candidate.clone());
        }
    }
    names.into_iter().collect()
}

/// Returns whether one physical slot of a class may be restamped as `mixed`.
fn slot_is_widenable(info: &ClassInfo, slot: usize) -> bool {
    let Some((property, _)) = info.properties.get(slot) else {
        return false;
    };
    let declared = info
        .property_declared_slots
        .get(slot)
        .copied()
        .unwrap_or_else(|| info.declared_properties.contains(property));
    if declared {
        return false;
    }
    // A reference slot is NOT skipped. `property_reference_slots` says the slot physically
    // holds a shared cell, while `properties[slot].1` says what that cell's PAYLOAD is, and an
    // undeclared property's payload is `mixed` whether or not an alias exists. Skipping it made
    // `$alias = &$o->u; clone($o, ["u" => "hello"]);` coerce the override back to the payload
    // type the default happened to infer. Declared slots already left above, so a declared typed
    // reference property keeps its declared payload type and its PHP weak-mode coercion and
    // type-error behavior.
    // A hooked property has no backing value of its own to widen, and its accessors carry
    // their own declared types.
    !info
        .property_hooks
        .get(property)
        .is_some_and(|hooks| hooks.any())
}

/// Returns whether a class owns its physical layout and must never be restamped.
///
/// Only a class this program DECLARES is widened. A checker-injected builtin (SPL, Reflection,
/// DateTime, the throwables) owns its slot representation together with the code that reads it,
/// and `clone()` on one of those is refused by the applicator planner anyway. Without this gate a
/// runtime-shaped two-argument `clone()` restamped builtin slots and an SPL constructor then
/// refused to lower its own declared `callable` parameter into its own `Callable` slot.
///
/// The builtin catalog, NOT `declared_classes`, is the authority for the injected classes: the
/// driver repopulates `declared_classes` from the class map AFTER builtin injection, so that set
/// contains the injected SPL, Reflection, DateTime and throwable classes too. A user subclass of a
/// builtin is absent from the catalog and stays eligible.
fn owns_physical_layout(checker: &Checker, class_name: &str) -> bool {
    elephc_builtin_contract::lookup_class(class_name).is_some()
        || !checker.declared_classes.contains(class_name)
        || checker.packed_classes.contains_key(class_name)
        || checker.enums.contains_key(class_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Wraps one argument expression in `...$arg`.
    fn spread(arg: Expr) -> Expr {
        Expr::new(ExprKind::Spread(Box::new(arg)), Span::dummy())
    }

    /// Wraps one argument expression in `name: $arg`.
    fn named(name: &str, arg: Expr) -> Expr {
        Expr::new(
            ExprKind::NamedArg {
                name: name.to_string(),
                value: Box::new(arg),
            },
            Span::dummy(),
        )
    }

    /// A one-argument invocation writes no property, so it must never widen any storage.
    ///
    /// This is the gate every recording path shares, including the runtime string callable one
    /// where the callee is not knowable and the widening would otherwise reach every user class.
    #[test]
    fn one_argument_clone_invocations_never_widen_property_storage() {
        assert!(!clone_call_may_carry_overrides(&[]));
        assert!(!clone_call_may_carry_overrides(&[Expr::var("object")]));
        assert!(!clone_call_may_carry_overrides(&[named(
            "object",
            Expr::var("object")
        )]));
    }

    /// Every shape that can carry `withProperties` records a widening destination.
    #[test]
    fn clone_invocations_that_can_carry_overrides_are_recorded() {
        assert!(clone_call_may_carry_overrides(&[
            Expr::var("object"),
            Expr::var("overrides"),
        ]));
        assert!(clone_call_may_carry_overrides(&[spread(Expr::var("args"))]));
        assert!(clone_call_may_carry_overrides(&[named(
            "withProperties",
            Expr::var("overrides")
        )]));
    }

    /// Builds one layout view whose every slot is widenable.
    fn view(parent: Option<&str>, slot_count: usize) -> LayoutView {
        LayoutView {
            parent: parent.map(str::to_string),
            widenable: vec![true; slot_count],
            owns_layout: false,
        }
    }

    /// Builds `Root` with one slot, two children that each add one, and one grandchild under the
    /// first child. `ChildA` and `ChildB` both have a slot at index 1, and they are NOT the same
    /// storage: one belongs to `ChildA`, the other to `ChildB`.
    fn sibling_views() -> BTreeMap<String, LayoutView> {
        BTreeMap::from([
            ("Root".to_string(), view(None, 1)),
            ("ChildA".to_string(), view(Some("Root"), 2)),
            ("ChildB".to_string(), view(Some("Root"), 2)),
            ("GrandA".to_string(), view(Some("ChildA"), 3)),
        ])
    }

    /// Verifies a destination selects its slots by their DECLARING owner, so a sibling's own slot
    /// at the same index is never selected.
    #[test]
    fn widening_selects_slots_by_declaring_owner_not_by_index_alone() {
        let selected = widened_slots(&sibling_views(), &["ChildA".to_string()]);
        assert_eq!(
            selected,
            BTreeSet::from([("Root".to_string(), 0), ("ChildA".to_string(), 1)])
        );
        assert!(
            !selected.iter().any(|(owner, _)| owner == "ChildB"),
            "a sibling's own storage must never be selected: {:?}",
            selected
        );
    }

    /// Verifies a shared ancestor slot reaches the whole subtree while a child's OWN slot stays in
    /// its own branch, which is what keeps `ChildB`'s unrelated slot 1 untouched.
    #[test]
    fn widening_propagates_a_shared_slot_but_not_a_branch_local_one() {
        let views = sibling_views();
        assert_eq!(
            slot_inheritors(&views, "Root", 0),
            BTreeSet::from([
                "ChildA".to_string(),
                "ChildB".to_string(),
                "GrandA".to_string(),
                "Root".to_string(),
            ])
        );
        assert_eq!(
            slot_inheritors(&views, "ChildA", 1),
            BTreeSet::from(["ChildA".to_string(), "GrandA".to_string()])
        );
    }

    /// Verifies a slot inherited from a class that owns its layout is never selected, so a user
    /// subclass of a catalog builtin widens its OWN slots and leaves the builtin's storage alone.
    #[test]
    fn widening_never_selects_a_slot_owned_by_an_authoritative_layout() {
        let views = BTreeMap::from([
            (
                "Catalog".to_string(),
                LayoutView {
                    parent: None,
                    widenable: vec![true],
                    owns_layout: true,
                },
            ),
            ("UserSub".to_string(), view(Some("Catalog"), 2)),
        ]);
        assert_eq!(
            widened_slots(&views, &["UserSub".to_string()]),
            BTreeSet::from([("UserSub".to_string(), 1)])
        );
    }
}
