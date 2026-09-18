//! Purpose:
//! Models throwable class and interface relationships for exception-aware optimization.
//! Combines checker metadata with source declarations needed for conservative constructor analysis.
//!
//! Called from:
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis`
//!
//! Key details:
//! - Symbol comparisons are case-insensitive while stored names retain their canonical spelling.
//! - Trait users form constructor barriers because a trait may supply the effective constructor.
//! - Destruction facts are recorded here too: whether the program holds a `__destruct` the
//!   summary collector cannot see, and which classes provably own no destructible storage.

use super::php_symbol_key;
use crate::parser::ast::{AttributeGroup, ClassProperty, Stmt, StmtKind, TypeExpr};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};
use std::collections::{HashMap, HashSet};

/// Canonical class/interface relations used to compare thrown and caught types.
#[derive(Clone, Debug, Default)]
pub(super) struct ExceptionHierarchy {
    pub(super) parents: HashMap<String, String>,
    interfaces: HashMap<String, HashSet<String>>,
    interface_parents: HashMap<String, HashSet<String>>,
    class_names: HashSet<String>,
    interface_names: HashSet<String>,
    declared_classes: HashSet<String>,
    trait_method_barriers: HashSet<String>,
    /// Classes proven to own no instance storage whose retirement can run a user destructor.
    scalar_only_storage_classes: HashSet<String>,
    /// Whether `scalar_only_storage_classes` entries already account for inherited storage.
    ///
    /// Checker metadata carries a FLATTENED property list, so a proof taken from it needs no
    /// parent walk. An AST-only proof describes one declaration, so the whole parent chain has
    /// to be proven separately before the class counts as destructible-storage free.
    flattened_storage_proofs: bool,
    /// Whether a `__destruct` body exists that the exception fixed point never summarizes.
    destructor_sources_open: bool,
}

impl ExceptionHierarchy {
    /// Builds authoritative hierarchy facts from type-checker metadata.
    pub(super) fn from_type_metadata(
        classes: &HashMap<String, ClassInfo>,
        interfaces: &HashMap<String, InterfaceInfo>,
        declared_classes: HashSet<String>,
    ) -> Self {
        let mut hierarchy = Self {
            declared_classes,
            flattened_storage_proofs: true,
            ..Self::default()
        };
        for (name, info) in classes {
            let key = php_symbol_key(name);
            hierarchy.class_names.insert(key.clone());
            if let Some(parent) = &info.parent {
                hierarchy.parents.insert(key.clone(), parent.clone());
            }
            if class_info_owns_only_scalar_storage(info) {
                hierarchy.scalar_only_storage_classes.insert(key.clone());
            }
            hierarchy
                .interfaces
                .insert(key, info.interfaces.iter().cloned().collect());
        }
        for (name, info) in interfaces {
            let key = php_symbol_key(name);
            hierarchy.interface_names.insert(key.clone());
            hierarchy
                .interface_parents
                .insert(key, info.parents.iter().cloned().collect());
        }
        hierarchy.add_throwable_roots();
        hierarchy
    }

    /// Builds best-effort hierarchy facts directly from AST declarations for public test helpers.
    pub(super) fn from_program(program: &[Stmt], declared_classes: HashSet<String>) -> Self {
        let mut hierarchy = Self {
            declared_classes,
            ..Self::default()
        };
        hierarchy.collect_program_declarations(program);
        hierarchy.add_throwable_roots();
        hierarchy
    }

    /// Adds the PHP root throwable relations needed even when no checker metadata is supplied.
    fn add_throwable_roots(&mut self) {
        self.interface_names.insert(php_symbol_key("Throwable"));
        for root in ["Exception", "Error"] {
            let key = php_symbol_key(root);
            self.class_names.insert(key.clone());
            self.interfaces
                .entry(key)
                .or_default()
                .insert("Throwable".to_string());
        }
    }

    /// Recursively collects class and interface declarations from namespace/grouping blocks.
    pub(super) fn collect_program_declarations(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    trait_uses,
                    properties,
                    ..
                } => {
                    let key = php_symbol_key(name);
                    self.class_names.insert(key.clone());
                    if !trait_uses.is_empty() {
                        self.trait_method_barriers.insert(key.clone());
                    }
                    // A checker-derived proof already covers inherited and promoted storage, so
                    // the weaker declaration-only proof must not be mixed into the same set.
                    if !self.flattened_storage_proofs
                        && declaration_owns_only_scalar_storage(
                            &stmt.attributes,
                            trait_uses.is_empty(),
                            properties,
                        )
                    {
                        self.scalar_only_storage_classes.insert(key.clone());
                    }
                    if let Some(parent) = extends {
                        self.parents.insert(key.clone(), parent.as_str().to_string());
                    }
                    self.interfaces.insert(
                        key,
                        implements
                            .iter()
                            .map(|name| name.as_str().to_string())
                            .collect(),
                    );
                }
                StmtKind::InterfaceDecl { name, extends, .. } => {
                    let key = php_symbol_key(name);
                    self.interface_names.insert(key.clone());
                    self.interface_parents.insert(
                        key,
                        extends
                            .iter()
                            .map(|name| name.as_str().to_string())
                            .collect(),
                    );
                }
                StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                    self.collect_program_declarations(body);
                }
                _ => {}
            }
        }
    }

    /// Returns whether `candidate` is the same as, extends, or implements `base`.
    pub(super) fn is_subtype(&self, candidate: &str, base: &str) -> bool {
        let candidate_key = php_symbol_key(candidate);
        let base_key = php_symbol_key(base);
        if candidate_key == base_key {
            return true;
        }
        if base_key == php_symbol_key("Throwable")
            && (self.class_names.contains(&candidate_key)
                || self.interface_names.contains(&candidate_key))
        {
            return self.type_reaches_interface(&candidate_key, &base_key)
                || self.class_reaches_root(&candidate_key, "Exception")
                || self.class_reaches_root(&candidate_key, "Error");
        }
        if self.class_reaches_root(&candidate_key, base) {
            return true;
        }
        self.type_reaches_interface(&candidate_key, &base_key)
    }

    /// Walks a class parent chain looking for `base`.
    fn class_reaches_root(&self, candidate_key: &str, base: &str) -> bool {
        let base_key = php_symbol_key(base);
        let mut current = Some(candidate_key.to_string());
        let mut seen = HashSet::new();
        while let Some(class_key) = current {
            if !seen.insert(class_key.clone()) {
                return false;
            }
            if class_key == base_key {
                return true;
            }
            current = self
                .parents
                .get(&class_key)
                .map(|parent| php_symbol_key(parent));
        }
        false
    }

    /// Walks class implementations, parent classes, and interface parents for a target interface.
    fn type_reaches_interface(&self, candidate_key: &str, base_key: &str) -> bool {
        let mut pending = vec![candidate_key.to_string()];
        let mut seen = HashSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if current == base_key {
                return true;
            }
            if let Some(parent) = self.parents.get(&current) {
                pending.push(php_symbol_key(parent));
            }
            if let Some(interfaces) = self.interfaces.get(&current) {
                pending.extend(interfaces.iter().map(|name| php_symbol_key(name)));
            }
            if let Some(parents) = self.interface_parents.get(&current) {
                pending.extend(parents.iter().map(|name| php_symbol_key(name)));
            }
        }
        false
    }

    /// Returns whether two upper-bound types can contain at least one common runtime class.
    pub(super) fn types_overlap(&self, left: &str, right: &str) -> bool {
        if self.is_subtype(left, right) || self.is_subtype(right, left) {
            return true;
        }
        let left_key = php_symbol_key(left);
        let right_key = php_symbol_key(right);
        if self.class_names.contains(&left_key) && self.class_names.contains(&right_key) {
            return false;
        }
        true
    }

    /// Returns whether a class came from user/source AST rather than injected builtin metadata.
    pub(super) fn is_declared_class(&self, class_name: &str) -> bool {
        self.declared_classes.contains(&php_symbol_key(class_name))
    }

    /// Returns whether a trait may provide a method absent from the class's explicit method list.
    pub(super) fn class_has_trait_method_barrier(&self, class_name: &str) -> bool {
        self.trait_method_barriers
            .contains(&php_symbol_key(class_name))
    }

    /// Returns whether magic-method lookup for a class is closed over known declarations.
    ///
    /// Closed means every link of the parent chain is a source-declared class or a builtin
    /// throwable, and no link opens a trait barrier. Constructor and destructor resolution
    /// share the property because they share the lookup: an ancestor the analysis never saw,
    /// or a trait that may supply the method, invalidates both equally.
    pub(super) fn method_lookup_is_closed(&self, class_name: &str) -> bool {
        let mut current = Some(php_symbol_key(class_name));
        let mut seen = HashSet::new();
        while let Some(class_key) = current {
            if !seen.insert(class_key.clone()) {
                return false;
            }
            if self.trait_method_barriers.contains(&class_key) {
                return false;
            }
            if !self.declared_classes.contains(&class_key)
                && !self.is_subtype(&class_key, "Throwable")
            {
                return false;
            }
            current = self
                .parents
                .get(&class_key)
                .map(|parent| php_symbol_key(parent));
        }
        true
    }

    /// Returns whether a `__destruct` body exists that the exception fixed point never summarizes.
    ///
    /// Trait-provided, body-less, and nested `__destruct` declarations never reach the
    /// instance-method summary map, so the program-wide destructor summary has to fall back to
    /// the unknown throwable domain whenever one of them exists.
    pub(super) fn destructor_sources_are_open(&self) -> bool {
        self.destructor_sources_open
    }

    /// Records whether destructor enumeration failed, keeping an earlier open verdict.
    ///
    /// Openness is a one-way latch: once any caller proves a destructor is unreachable to the
    /// summary collector, a later scan must not be able to claim enumeration succeeded.
    pub(super) fn mark_destructor_sources_open(&mut self, open: bool) {
        self.destructor_sources_open |= open;
    }

    /// Returns whether retiring an instance of `class_name` provably runs no property destructor.
    ///
    /// Destroying an object retires the instance storage it owns: declared properties,
    /// inherited properties, promoted constructor properties, dynamic/eval-visible side-table
    /// properties, and the container children those hold, before and after its own `__destruct`
    /// body runs. Only a proof that every one of those slots is a non-heap scalar removes that
    /// term; anything unproven keeps the caller on the conservative program-wide destructor
    /// summary.
    pub(super) fn class_owns_no_destructible_storage(&self, class_name: &str) -> bool {
        if self.flattened_storage_proofs {
            return self
                .scalar_only_storage_classes
                .contains(&php_symbol_key(class_name));
        }
        let mut current = Some(php_symbol_key(class_name));
        let mut seen = HashSet::new();
        while let Some(class_key) = current {
            if !seen.insert(class_key.clone()) {
                return false;
            }
            if !self.scalar_only_storage_classes.contains(&class_key) {
                return false;
            }
            current = self
                .parents
                .get(&class_key)
                .map(|parent| php_symbol_key(parent));
        }
        true
    }
}

/// Returns whether a checker-described class owns only non-heap scalar instance storage.
///
/// `ClassInfo::properties` is the FLATTENED instance layout, so inherited and promoted slots
/// are already included. An owned reference property allocates a cell the object frees on
/// destruction, so it is never part of a scalar-only proof.
///
/// The declared layout is not the whole story. `ClassInfo::has_property_hash_storage()` is true
/// for a class carrying (or inheriting) `#[\AllowDynamicProperties]` and for one the checker
/// gave eval-visible property storage: both add a side table that can hold an object the
/// declared property list never mentions, so a class with either is refused outright.
fn class_info_owns_only_scalar_storage(info: &ClassInfo) -> bool {
    !info.has_property_hash_storage()
        && info.owned_reference_properties.is_empty()
        && info
            .properties
            .iter()
            .all(|(_, property_type)| php_type_is_non_destructible(property_type))
}

/// Returns whether a PHP type's storage can never hold a value with a user destructor.
///
/// Objects, `mixed`, `iterable`, `callable`, arrays, and unions over any of those can hold an
/// object, so only the closed set of non-heap scalars and compiler-internal storage answers
/// true. Arrays answer false because retiring a container retires the children it owns.
fn php_type_is_non_destructible(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::TaggedScalar => true,
        PhpType::Union(members) => members.iter().all(php_type_is_non_destructible),
        PhpType::Object(_)
        | PhpType::Mixed
        | PhpType::Iterable
        | PhpType::Callable
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => false,
    }
}

/// Returns whether one class DECLARATION owns only non-heap scalar instance storage.
///
/// This is the weaker AST-only proof used when no checker metadata is available. It describes a
/// SINGLE declaration, so [`ExceptionHierarchy::class_owns_no_destructible_storage`] still walks
/// the parent chain and requires every link to be proven the same way, which is also what makes
/// an unknown or dynamic-storage ancestor refuse the whole chain.
///
/// It refuses a class that uses a trait (a trait may add properties), an untyped, by-reference,
/// or non-scalar property, and a class carrying `#[\AllowDynamicProperties]`, whose instances
/// can hold an object under a name no declaration mentions. Constructor-promoted properties need
/// no separate check: the parser appends them to `properties` with their declared type, so the
/// property scan already covers them.
fn declaration_owns_only_scalar_storage(
    attributes: &[AttributeGroup],
    has_no_traits: bool,
    properties: &[ClassProperty],
) -> bool {
    if !has_no_traits || declaration_allows_dynamic_properties(attributes) {
        return false;
    }
    properties.iter().all(|property| {
        property.is_static
            || (!property.by_ref
                && property
                    .type_expr
                    .as_ref()
                    .is_some_and(type_expr_is_non_destructible))
    })
}

/// Returns whether a class declaration carries PHP 8.2 `#[\AllowDynamicProperties]`.
///
/// Attribute names are class names, so the comparison is case-insensitive on the unqualified
/// last segment: `#[AllowDynamicProperties]` and `#[\AllowDynamicProperties]` are the same
/// marker. Matching a user attribute that merely ends in that name would only refuse a proof,
/// which costs precision rather than soundness.
fn declaration_allows_dynamic_properties(attributes: &[AttributeGroup]) -> bool {
    attributes.iter().any(|group| {
        group.attributes.iter().any(|attribute| {
            attribute
                .name
                .last_segment()
                .is_some_and(|segment| segment.eq_ignore_ascii_case("AllowDynamicProperties"))
        })
    })
}

/// Returns whether a source type annotation names a non-heap scalar with no destructor.
///
/// Only the scalar keyword forms answer true. A nullable scalar still stores a scalar or null,
/// so it qualifies; every named class-like, array, buffer, pointer, iterable, or intersection
/// position does not, and an unannotated position never reaches this function at all. The
/// `match` is exhaustive so a new `TypeExpr` has to be classified rather than silently
/// defaulting to "cannot hold an object".
pub(super) fn type_expr_is_non_destructible(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never => true,
        TypeExpr::Nullable(inner) => type_expr_is_non_destructible(inner),
        TypeExpr::Union(members) => members.iter().all(type_expr_is_non_destructible),
        TypeExpr::Named(_)
        | TypeExpr::Iterable
        | TypeExpr::Array(_)
        | TypeExpr::Ptr(_)
        | TypeExpr::Buffer(_)
        | TypeExpr::Intersection(_) => false,
    }
}
