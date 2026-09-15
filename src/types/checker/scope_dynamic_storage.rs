//! Purpose:
//! Reserves the per-instance property hash a class needs when a reachable mutation can create a
//! dynamic property, either through a runtime name or through a strict ancestor's private name.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments::properties`, for `$o->p = v` and every
//!   form that desugars to it (compound assignment, pre/post increment and decrement).
//! - `crate::types::checker::builtins::language_constructs`, for `unset($o->p)`.
//! - `crate::types::checker::check_types_with_options`, which applies the reservation once every
//!   body has been checked.
//!
//! Key details:
//! - php 7.4 removed shadow properties. A strict ancestor's `private $p` lives under a mangled
//!   key, and the child's by-name table does not contain it at all, so `$child->p = 1` outside
//!   the declaring class CREATES a distinct dynamic property and leaves the ancestor's slot
//!   untouched. This compiler's `ClassInfo::properties` is the PHYSICAL slot table, which still
//!   carries that slot under its plain name, so without a hash the backend's by-name ladder falls
//!   through to `resolve_property_slot_for_class` and writes the ANCESTOR'S storage. Reserving
//!   the hash is what gives the distinct dynamic property somewhere to live.
//! - A runtime-name write can miss every declared property, so its static receiver class and all
//!   reachable subclasses need storage. Classes with `__set` need it too because PHP suppresses
//!   only an active receiver/name pair and stores same-pair reentry in the dynamic hash.
//! - The reservation is PROGRAM-USAGE gated, exactly like
//!   [`super::clone_override_storage`]'s clone destinations. A class no reachable mutation
//!   addresses that way pays nothing: the trailing pointer per instance, and the clone/free/GC
//!   traversal `#[\AllowDynamicProperties]` instances already pay, are charged only to the
//!   classes a site in THIS program can actually reach.
//! - Recording is done on the STATIC receiver class and expanded over its subclasses, because a
//!   parameter typed `Child` can hold a `GrandChild` at run time and the write must find storage
//!   on whatever class the instance really is.
//! - The flag is a STORAGE capability, never php's permission.
//!   `allow_dynamic_properties` stays exactly as the attribute left it, so
//!   `ClassInfo::dynamic_property_creation_is_deprecated()` still separates php 8.5's exempt
//!   classes from the deprecated ones: an ordinary class keeps reporting
//!   `Creation of dynamic property C::$p is deprecated`, and an
//!   `#[\AllowDynamicProperties]` class keeps reporting nothing.

use std::collections::BTreeSet;

use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::Checker;

/// One recorded mutation site, kept whole so the subclass expansion can re-ask php's question.
///
/// The class alone is not enough. Expansion has to decide, per runtime subclass, whether php
/// would really create a dynamic property there. For a static name that depends on the name and
/// lexical scope. For a runtime name, every class can receive an undeclared name, including a
/// class with `__set` when the same receiver/name pair reenters that accessor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScopeDynamicMutationSite {
    /// The receiver class the site names, before expansion.
    class_name: String,
    /// The exact property name, or `None` when the site spells the name at run time.
    property: Option<String>,
    /// The accessor php consults FIRST: `__set` for a write, `__unset` for an `unset()`.
    magic_method: String,
    /// The lexical scope the site was written in, used to resolve a static property name.
    scope: Option<String>,
    /// This static-name write occurs through `$this` inside `__set`, so invocation with the same
    /// name bypasses the accessor and materializes the property in dynamic storage.
    same_pair_set_reentry: bool,
}

/// The mutation sites in this program that can create a dynamic property.
#[derive(Debug, Default, Clone)]
pub struct ScopeDynamicMutationTargets {
    sites: BTreeSet<ScopeDynamicMutationSite>,
}

impl ScopeDynamicMutationTargets {
    /// Returns whether this program reaches such a mutation at all.
    fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

/// Returns whether php resolves `property` on `class_name` to a DYNAMIC property in this scope
/// while the physical layout still carries a slot of that name.
///
/// The single question the mutation checks ask before they decide anything: it is what separates
/// a write php turns into a distinct dynamic property from one it refuses or sends to a slot.
pub(in crate::types::checker) fn mutation_targets_scope_dynamic_name(
    checker: &Checker,
    class_name: &str,
    property: &str,
) -> bool {
    crate::types::property_name_shadows_ancestor_private_slot(
        &checker.classes,
        class_name,
        property,
        checker.current_class.as_deref(),
    )
}

/// Records one mutation site whose name php resolves to a dynamic property on `class_name`.
///
/// `magic_method` is the accessor php consults FIRST for this mutation: `__set` for a write,
/// `__unset` for an `unset`. A class that declares it never reaches the hash at all, because php
/// hands the name to the accessor instead of creating anything, so such a class is not charged
/// the storage. This is the same precedence `magic_set_receiver_has_method` applies in lowering.
///
/// The site is recorded WHOLE and that precedence is applied later, by `site_needs_storage_on_class`
/// during the subclass expansion. Applying it only here would have charged every subclass that
/// declares the accessor its parent lacks, which is the polymorphic case `crate::ir_lower` peels
/// off with an `instanceof` guard and never lets reach a hash at all.
pub(in crate::types::checker) fn record_scope_dynamic_mutation(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    magic_method: &str,
) {
    if !mutation_targets_scope_dynamic_name(checker, class_name, property) {
        return;
    }
    record_site(
        checker,
        class_name,
        Some(property.to_string()),
        magic_method,
        false,
    );
}

/// Inserts one site, normalized, with the scope it was written in.
fn record_site(
    checker: &mut Checker,
    class_name: &str,
    property: Option<String>,
    magic_method: &str,
    same_pair_set_reentry: bool,
) {
    let site = ScopeDynamicMutationSite {
        class_name: class_name.trim_start_matches('\\').to_string(),
        property,
        magic_method: magic_method.to_string(),
        scope: checker.current_class.clone(),
        same_pair_set_reentry,
    };
    checker.scope_dynamic_mutation_targets.sites.insert(site);
}

/// Records one RUNTIME-name mutation site (`$o->{$name} = v`) on a statically known class.
///
/// The name is not known here, so it can miss every declared property and create a dynamic one.
/// Recording the static class lets expansion reserve the same reachable runtime-class subtree the
/// backend dispatches over. A class with `__set` still needs the hash for same-pair reentry.
pub(in crate::types::checker) fn record_scope_dynamic_runtime_name_mutation(
    checker: &mut Checker,
    class_name: &str,
) {
    let normalized = class_name.trim_start_matches('\\');
    if !checker.classes.contains_key(normalized) {
        return;
    }
    let normalized = normalized.to_string();
    record_site(checker, &normalized, None, "__set", false);
}

/// Records one RUNTIME-name mutation site for ANY receiver shape the checker admits.
///
/// `$o->{$k} = v` accepts an `Object`, a `Union` of them and a boxed `Mixed`, and each one has its
/// own backend ladder: `lower_runtime_object_prop_set` for the first and
/// `lower_runtime_mixed_prop_set` for the other two. All three dispatch on the RUNTIME class, so
/// the reservation has to cover the same set of classes the ladder can select, or an arm addresses
/// a hash the class never reserved.
pub(in crate::types::checker) fn record_scope_dynamic_runtime_name_receiver_mutation(
    checker: &mut Checker,
    receiver_ty: &PhpType,
) {
    match receiver_ty {
        PhpType::Object(class_name) => {
            let class_name = class_name.clone();
            record_scope_dynamic_runtime_name_mutation(checker, &class_name);
        }
        PhpType::Union(members) => {
            // A union that resolves to ONE object class is that class, and degrading it to "every
            // class in the program" would charge storage to classes this site can never reach.
            // `union_single_object_class` is the same authority `unset()`'s own receiver check
            // uses, so the two agree about what a union receiver really is.
            if let Some(class_name) = checker.union_single_object_class(receiver_ty) {
                record_scope_dynamic_runtime_name_mutation(checker, &class_name);
                return;
            }
            // Otherwise the union still NAMES its object members, so each of those is recorded on
            // its own. Only a `Mixed` member means the receiver can truly be any object.
            for member in members.clone() {
                match member {
                    PhpType::Object(_) | PhpType::Union(_) => {
                        record_scope_dynamic_runtime_name_receiver_mutation(checker, &member)
                    }
                    PhpType::Mixed => {
                        record_scope_dynamic_runtime_name_mixed_receiver_mutation(checker)
                    }
                    _ => {}
                }
            }
        }
        // A boxed `Mixed` names no class, so the sound record is every class the runtime-class
        // ladder can select. Program-usage gating still keeps this cost absent from programs with
        // no runtime-name write through a Mixed receiver.
        PhpType::Mixed => record_scope_dynamic_runtime_name_mixed_receiver_mutation(checker),
        _ => {}
    }
}

/// Records every class a boxed `Mixed` runtime-name mutation can land on.
///
/// The receiver names no static class, and an unknown property name can miss every declared slot,
/// so every runtime class is a possible dynamic-property destination. An accessor does not remove
/// that need because its active receiver/name pair can reenter and create the property.
fn record_scope_dynamic_runtime_name_mixed_receiver_mutation(checker: &mut Checker) {
    let targets = checker
        .classes
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for class_name in targets {
        record_scope_dynamic_runtime_name_mutation(checker, &class_name);
    }
}

/// Records one STATIC-name mutation site whose receiver is only known to be runtime-shaped.
///
/// A boxed `Mixed` receiver names no class, so the sound record is every class whose layout
/// carries THIS name as a strict ancestor's private slot. That is already a narrow set: an
/// ordinary undeclared name matches nothing at all, and a name only some classes shadow charges
/// only those. The Mixed write ladder dispatches on the runtime class id, so each recorded class
/// gets its own arm addressing its own hash.
pub(in crate::types::checker) fn record_scope_dynamic_mixed_receiver_mutation(
    checker: &mut Checker,
    property: &str,
) {
    let scope = checker.current_class.clone();
    let targets = checker
        .classes
        .iter()
        .filter(|(class_name, class_info)| {
            !class_info.methods.contains_key("__set")
                && crate::types::property_name_shadows_ancestor_private_slot(
                    &checker.classes,
                    class_name,
                    property,
                    scope.as_deref(),
                )
        })
        .map(|(class_name, _)| class_name.clone())
        .collect::<Vec<_>>();
    for class_name in targets {
        record_site(
            checker,
            &class_name,
            Some(property.to_string()),
            "__set",
            false,
        );
    }
}

/// Records a literal `$this->name = value` in `__set` as a possible same-pair reentry store.
///
/// PHP suppresses recursive dispatch only when the receiver and property name match the active
/// `__set` invocation. Such a write creates a dynamic property even though the class declares
/// `__set`. Restricting this record to `$this`, the current class and one literal name keeps the
/// later read exemption from admitting unrelated missing properties.
pub(in crate::types::checker) fn record_magic_set_same_pair_reentry(
    checker: &mut Checker,
    object: &Expr,
    class_name: &str,
    property: &str,
) -> bool {
    if !matches!(object.kind, ExprKind::This)
        || checker.current_method.as_deref() != Some("__set")
        || !checker
            .current_class
            .as_deref()
            .is_some_and(|scope| scope == class_name)
    {
        return false;
    }
    if crate::types::resolve_property_name(
        &checker.classes,
        class_name,
        property,
        checker.current_class.as_deref(),
    ) != crate::types::PropertyNameResolution::Dynamic
    {
        return false;
    }
    record_site(
        checker,
        class_name,
        Some(property.to_string()),
        "__set",
        true,
    );
    true
}

/// Returns whether an exact property name can be materialized by same-pair `__set` reentry.
///
/// Body checking records the site before the final top-level pass reads it. The class storage
/// flag is installed just after all checking completes, so the exact recorded site is also the
/// pre-reservation proof that the class will receive that storage. The name match remains
/// mandatory in either state and prevents one reentry store from opening every missing name.
pub(in crate::types::checker) fn magic_set_reentry_property_is_readable(
    checker: &Checker,
    class_name: &str,
    property: &str,
) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    let storage_installed = checker
        .classes
        .get(class_name)
        .is_some_and(|info| info.scope_dynamic_property_storage);

    checker.scope_dynamic_mutation_targets.sites.iter().any(|site| {
        site.same_pair_set_reentry
            && site.property.as_deref() == Some(property)
            && (site.class_name == class_name
                || checker.is_subclass_of(class_name, &site.class_name))
            && (storage_installed || site_needs_storage_on_class(checker, site, class_name))
    })
}

/// Reserves the per-instance property hash on every class a recorded mutation can reach.
///
/// Runs after every body has been checked, so the recorded set is complete and the class map has
/// its final parent links for the subclass expansion.
pub(in crate::types::checker) fn reserve_scope_dynamic_property_storage(checker: &mut Checker) {
    let targets = std::mem::take(&mut checker.scope_dynamic_mutation_targets);
    if targets.is_empty() && !checker.program_contains_eval {
        return;
    }
    let mut classes = target_classes(checker, &targets)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if checker.program_contains_eval {
        classes.extend(checker.classes.keys().cloned());
    }
    for class_name in classes {
        reserve_property_hash_storage(checker, &class_name);
    }
}

/// Expands the recorded sites into the exact set of classes to reserve storage on.
///
/// A receiver typed `Child` can hold any subclass of `Child` at run time, so every subclass is
/// considered. A static-name site re-asks php's scope question on each candidate because a
/// subclass can redeclare the name. A runtime-name site reserves every candidate because the name
/// can miss every declared slot, and same-pair `__set` reentry creates dynamic storage.
fn target_classes(checker: &Checker, targets: &ScopeDynamicMutationTargets) -> Vec<String> {
    let mut names = BTreeSet::new();
    for site in &targets.sites {
        for (candidate, _) in checker.classes.iter() {
            if candidate != &site.class_name && !checker.is_subclass_of(candidate, &site.class_name)
            {
                continue;
            }
            if site_needs_storage_on_class(checker, site, candidate) {
                names.insert(candidate.clone());
            }
        }
    }
    names.into_iter().collect()
}

/// Returns whether php would really create a dynamic property for this SITE on this CLASS.
///
/// For a STATIC name, an accessor handles the operation before a hash is needed. A runtime-name
/// site has no single name and can always miss the declared-name ladder, while a same-pair
/// accessor reentry writes to the hash, so it needs storage on every eligible class.
fn site_needs_storage_on_class(
    checker: &Checker,
    site: &ScopeDynamicMutationSite,
    class_name: &str,
) -> bool {
    let Some(class_info) = checker.classes.get(class_name) else {
        return false;
    };
    if site.property.is_some()
        && !site.same_pair_set_reentry
        && class_info.methods.contains_key(site.magic_method.as_str())
    {
        return false;
    }
    if site.same_pair_set_reentry {
        return site.property.as_deref().is_some_and(|property| {
            crate::types::resolve_property_name(
                &checker.classes,
                class_name,
                property,
                site.scope.as_deref(),
            ) == crate::types::PropertyNameResolution::Dynamic
        });
    }
    match &site.property {
        Some(property) => crate::types::property_name_shadows_ancestor_private_slot(
            &checker.classes,
            class_name,
            property,
            site.scope.as_deref(),
        ),
        None => true,
    }
}

/// Sets the storage flag on one class, skipping every class that owns its physical layout.
///
/// Same exclusions as `clone_override_storage::reserve_property_hash_storage`: a checker-injected
/// builtin, a packed class and an enum all own their slot representation together with the code
/// that reads it, and stdClass already stores every property in a hash.
///
/// The builtin catalog, NOT `declared_classes`, is the authority for the first of those. The
/// driver repopulates `declared_classes` from the class map AFTER builtin injection, so that set
/// contains the injected SPL, Reflection, DateTime and throwable classes too and would exclude
/// nothing here. A user subclass of a builtin is absent from the catalog and stays eligible.
///
/// A `readonly` class is excluded for a different reason, php's own: it carries the engine's
/// no-dynamic-properties flag, so no write may ever create an entry there and the write checks
/// refuse the creation outright. Reserving a hash it can never legally fill would charge every
/// instance a trailing pointer plus clone and GC traversal for nothing.
fn reserve_property_hash_storage(checker: &mut Checker, class_name: &str) {
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
    if info.is_readonly_class {
        return;
    }
    if info.has_property_hash_storage() {
        return;
    }
    info.scope_dynamic_property_storage = true;
}
