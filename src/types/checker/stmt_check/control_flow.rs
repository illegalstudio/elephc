//! Purpose:
//! Validates statement control flow behavior.
//! Keeps control-flow and assignment effects synchronized with expression inference and return analysis.
//!
//! Called from:
//! - `crate::types::checker::stmt_check`
//!
//! Key details:
//! - Branch and loop handling must preserve PHP execution order and conservative type environments.
//! - A `foreach` over a PHP-visible non-iterable (`int`, `string`, `bool`, `null`, `float`,
//!   `resource`) is a WARNING, not an error: php-src raises
//!   `foreach() argument must be of type array|object, <type> given` at runtime and keeps
//!   going, so rejecting it would make elephc refuse a program PHP runs. Codegen emits the
//!   matching runtime warning and skips the loop
//!   (`IteratorSourceKind::NonIterable` in `crate::codegen::lower_inst::iterators`).
//!   Compiler-internal types with no PHP spelling stay a hard error.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, property_hook_get_method};
use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::parser::ast::{
    BinOp, CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind, Visibility,
};
use crate::termination::{block_terminal_effect_with_divergence, TerminalEffect};
use crate::types::{PhpType, TypeEnv};

use super::super::{Checker, SavedNarrowingOrigin};
use super::loop_entry::stabilize_loop_storage;

const FS_CURRENT_AS_SELF: i64 = 16;
const FS_CURRENT_AS_PATHNAME: i64 = 32;
const FS_CURRENT_MODE_MASK: i64 = 240;
const FS_SKIP_DOTS: i64 = 4096;

impl Checker {
    /// Forgets every loop-storage contract recorded for `scope`, before its body is walked again.
    pub(crate) fn clear_loop_storage_scope(&mut self, scope: &str) {
        if let Some(spans) = self.loop_storage_scope_spans.remove(scope) {
            for span in spans {
                self.loop_storage_types.remove(&(scope.to_string(), span));
            }
        }
    }
}

/// Gives a by-reference foreach root its persistent hash representation.
///
/// PHP's `array` property declaration constrains the container, not its element types. Once a
/// by-reference loop can replace an element with a different runtime type, every later property
/// read must select boxed Mixed hash operations rather than the initializer's old concrete
/// layout. Nested sources promote their root too, then traverse boxed Mixed entries dynamically.
/// Applying the storage change to property metadata also converts initializers and assignments,
/// so no `Array`-typed access can observe hash-backed storage.
fn promoted_by_ref_foreach_root_type(ty: &PhpType) -> Option<PhpType> {
    let ty = ty.codegen_repr();
    match ty {
        PhpType::Array(_) => Some(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        }),
        PhpType::AssocArray { key, .. } => Some(PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        }),
        _ => None,
    }
}

/// Widens local or static property storage when foreach exposes mutable element references.
pub(super) fn widen_by_ref_foreach_source_storage(
    checker: &mut Checker,
    source: &Expr,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let mut root = source;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind {
        root = array;
    }
    if let ExprKind::Variable(name) = &root.kind {
        if let Some(widened) = env
            .get(name)
            .and_then(promoted_by_ref_foreach_root_type)
        {
            env.insert(name.clone(), widened);
        }
        return Ok(());
    }
    if let ExprKind::StaticPropertyAccess { receiver, property } = &root.kind {
        let access_class = checker.resolve_static_property_receiver(receiver, root)?;
        let Some(declaring_class) = checker
            .classes
            .get(&access_class)
            .and_then(|info| info.static_property_declaring_classes.get(property).cloned())
        else {
            return Ok(());
        };
        let mut reachable_declarations = HashSet::from_iter([declaring_class]);
        if matches!(receiver, StaticReceiver::Static) {
            for (class_name, info) in &checker.classes {
                if class_name != &access_class
                    && !checker.is_subclass_of(class_name, &access_class)
                {
                    continue;
                }
                let Some(selected_declaration) =
                    info.static_property_declaring_classes.get(property)
                else {
                    continue;
                };
                let visibility = info
                    .static_property_visibilities
                    .get(property)
                    .unwrap_or(&Visibility::Public);
                // The backend's late-static dispatch raises before loading a private
                // redeclaration owned by a descendant, so that inaccessible storage must keep
                // its independently declared representation.
                if matches!(visibility, Visibility::Private)
                    && selected_declaration != &access_class
                {
                    continue;
                }
                reachable_declarations.insert(selected_declaration.clone());
            }
        }
        for info in checker.classes.values_mut() {
            if !info
                .static_property_declaring_classes
                .get(property)
                .is_some_and(|declaring| reachable_declarations.contains(declaring))
            {
                continue;
            }
            let Some((_, property_ty)) = info
                .static_properties
                .iter_mut()
                .find(|(name, _)| name == property)
            else {
                continue;
            };
            if let Some(widened) = promoted_by_ref_foreach_root_type(property_ty) {
                *property_ty = widened;
            }
        }
        return Ok(());
    }
    let ExprKind::PropertyAccess { object, property } = &root.kind else {
        return Ok(());
    };
    let object_ty = checker.infer_type(object, env)?;
    let Some(access_class) = crate::types::checker::single_object_class_name(&object_ty) else {
        return Ok(());
    };
    let access_class = access_class.trim_start_matches('\\').to_string();
    let property_is_addressable = checker.classes.get(&access_class).is_some_and(|info| {
        !info
            .methods
            .contains_key(&php_symbol_key(&property_hook_get_method(property)))
            && matches!(
                crate::types::resolve_property_name(
                    &checker.classes,
                    &access_class,
                    property,
                    checker.current_class.as_deref(),
                ),
                crate::types::PropertyNameResolution::Visible
                    | crate::types::PropertyNameResolution::ScopePrivate { .. }
            )
    });
    if !property_is_addressable {
        return Ok(());
    }
    checker
        .reference_property_promotions
        .insert((access_class.clone(), property.clone()));
    let Some(declaring_class) = checker
        .classes
        .get(&access_class)
        .and_then(|info| info.property_declaring_classes.get(property).cloned())
    else {
        return Ok(());
    };
    for info in checker.classes.values_mut() {
        if !info
            .property_declaring_classes
            .get(property)
            .is_some_and(|declaring| declaring == &declaring_class)
        {
            continue;
        }
        let Some(slot) = info.visible_property_index(property) else {
            continue;
        };
        let widened = promoted_by_ref_foreach_root_type(&info.properties[slot].1);
        if let Some(widened) = widened {
            info.properties[slot].1 = widened;
        }
    }
    Ok(())
}

/// Gives `$object->property` the storage an array element can share: an object-owned reference
/// cell holding a boxed `Mixed`.
///
/// `$a[$k] = &$o->p` and `[&$this->p]` store a marker over the property's cell, and the marker
/// writes boxed values through it (`ArrayLocalRefCell`). A property still stored as its concrete
/// representation read those boxes back as raw payloads -- `$a['r'] = 7; echo $o->p;` printed a
/// pointer. An untyped property (or one whose declared type already is `Mixed` storage) is
/// widened; a scalar-typed one keeps its type, the marker carrying the cell's tag instead; returns
/// `false` for any other, which the caller refuses.
pub(in crate::types::checker) fn widen_property_for_element_reference(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    env: &TypeEnv,
) -> Result<bool, CompileError> {
    let object_ty = checker.infer_type(object, env)?;
    let Some(access_class) = crate::types::checker::single_object_class_name(&object_ty) else {
        return Ok(widen_property_for_unknown_receiver_reference(checker, property));
    };
    let access_class = access_class.trim_start_matches('\\').to_string();
    let Some(info) = checker.classes.get(&access_class) else {
        return Ok(false);
    };
    let Some(declaring_class) = info.property_declaring_classes.get(property).cloned() else {
        return Ok(false);
    };
    let Some(slot) = info.visible_property_index(property) else {
        return Ok(false);
    };
    let declared = info.declared_properties.contains(property);
    let widenable = !declared || info.properties[slot].1.codegen_repr() == PhpType::Mixed;
    // A scalar-typed property is widened too (`private int $cursor`, which Symfony's
    // `OrderedHashMapIterator` shares as `$this->managedCursors[$id] = &$this->cursor`): the
    // element markers' readers (the `array_splice()` prelude among them) only follow boxed cells.
    // The one php behaviour this gives up is the TypeError for a wrong-typed write through the
    // reference.
    // A nullable scalar (`private ?int $remaining`, a tagged scalar) and an array
    // (`private array $info`) are widened the same way: symfony/http-client's `NativeResponse`
    // shares both with `[&$this->pauseExpiry, ..., &$this->remaining, &$this->info, ...]`.
    let typed_scalar = declared
        && matches!(
            info.properties[slot].1.codegen_repr(),
            PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::Str
                | PhpType::TaggedScalar
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
        );
    let addressable = !info
        .methods
        .contains_key(&php_symbol_key(&property_hook_get_method(property)))
        && matches!(
            crate::types::resolve_property_name(
                &checker.classes,
                &access_class,
                property,
                checker.current_class.as_deref(),
            ),
            crate::types::PropertyNameResolution::Visible
                | crate::types::PropertyNameResolution::ScopePrivate { .. }
        );
    if !(widenable || typed_scalar) || !addressable {
        return Ok(false);
    }
    checker
        .reference_property_promotions
        .insert((access_class, property.to_string()));
    for info in checker.classes.values_mut() {
        if info.property_declaring_classes.get(property) != Some(&declaring_class) {
            continue;
        }
        if let Some(slot) = info.visible_property_index(property) {
            info.properties[slot].1 = PhpType::Mixed;
        }
    }
    Ok(true)
}

/// Gives `$property` boxed `Mixed` reference-cell storage on every class that declares it, for a
/// reference whose receiver class is only known at run time (`$t = &$object->n` with an untyped
/// `$object`).
///
/// The reference is read and written as `Mixed`, and the class-id ladder that resolves its cell
/// can land on any declaring class, so every one of them has to store a box: a cell left as a raw
/// `int` received the box pointer itself (`echo $a->n` printed an address). Classes whose
/// `$property` is hooked or not visible from this scope keep their storage and get no arm, which
/// raises php's no-cell `Error` at run time. As for element references, the one php behaviour
/// given up is the TypeError for a wrong-typed write through the reference. Returns whether any
/// class was widened.
pub(in crate::types::checker) fn widen_property_for_unknown_receiver_reference(
    checker: &mut Checker,
    property: &str,
) -> bool {
    widen_properties_for_unknown_receiver_reference(checker, |name| name == property)
}

/// Widens every property whose name satisfies `selects` the way
/// [`widen_property_for_unknown_receiver_reference`] widens one, for a reference whose NAME is
/// also only known at run time (`$r = &$object->$name`, Symfony's `PropertyAccessor`). Readonly
/// properties are left alone: php refuses a reference to them.
pub(in crate::types::checker) fn widen_properties_for_unknown_receiver_reference(
    checker: &mut Checker,
    selects: impl Fn(&str) -> bool,
) -> bool {
    widen_properties_for_receiver_reference(checker, None, selects)
}

/// [`widen_properties_for_unknown_receiver_reference`] limited to `receivers` (a known class
/// and its subclasses) when given: `&$known->$name` can only reach a property of the runtime
/// class, so widening every class in scope would change layouts the receiver never has.
pub(in crate::types::checker) fn widen_properties_for_receiver_reference(
    checker: &mut Checker,
    receivers: Option<&HashSet<String>>,
    selects: impl Fn(&str) -> bool,
) -> bool {
    let mut selected = Vec::new();
    for (class_name, info) in &checker.classes {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name)
            || receivers.is_some_and(|receivers| !receivers.contains(class_name))
        {
            continue;
        }
        for (property, _) in &info.properties {
            if !selects(property)
                || info.readonly_properties.contains(property)
                || info
                    .methods
                    .contains_key(&php_symbol_key(&property_hook_get_method(property)))
                || !matches!(
                    crate::types::resolve_property_name(
                        &checker.classes,
                        class_name,
                        property,
                        checker.current_class.as_deref(),
                    ),
                    crate::types::PropertyNameResolution::Visible
                        | crate::types::PropertyNameResolution::ScopePrivate { .. }
                )
            {
                continue;
            }
            let Some(declaring) = info.property_declaring_classes.get(property) else {
                continue;
            };
            selected.push((class_name.clone(), property.clone(), declaring.clone()));
        }
    }
    if selected.is_empty() {
        return false;
    }
    let declarers = selected
        .iter()
        .map(|(_, property, declaring)| (property.clone(), declaring.clone()))
        .collect::<HashSet<_>>();
    for (class_name, property, _) in selected {
        if checker.is_native_throwable_payload_property(&class_name, &property) {
            continue;
        }
        checker
            .reference_property_promotions
            .insert((class_name, property));
    }
    for info in checker.classes.values_mut() {
        for slot in 0..info.properties.len() {
            let property = &info.properties[slot].0;
            let shares_declarer = info
                .property_declaring_classes
                .get(property)
                .is_some_and(|declaring| declarers.contains(&(property.clone(), declaring.clone())));
            if shares_declarer && info.visible_property_index(property) == Some(slot) {
                info.properties[slot].1 = PhpType::Mixed;
            }
        }
    }
    true
}

/// Widens a local array to the storage a reference into one of its (possibly nested) elements
/// needs, before the reference's own type is inferred.
///
/// `$cell = &$nested['x']['y']` auto-vivifies `$nested['x']` and makes the leaf a reference
/// marker that can later hold a value of ANY type. The checker therefore has to stop describing
/// the container with the initializer's narrow shape: an empty literal (`array<never>`) has no
/// element type to index into, and a hash of fixed-type rows would read the marker back as a raw
/// value. A chain of two or more levels promotes its root to boxed `Mixed` values so every
/// intermediate level reads as `Mixed` and dispatches on the runtime tag; a single level over an
/// empty literal takes the shape its first key selects, as an ordinary keyed write does.
///
/// Only a plain local root is handled here; property and static-property roots go through
/// [`widen_by_ref_foreach_source_storage`].
pub(super) fn widen_local_root_for_element_reference(
    checker: &mut Checker,
    source: &Expr,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let ExprKind::ArrayAccess { array, .. } = &source.kind else {
        return Ok(());
    };
    let nested = matches!(array.kind, ExprKind::ArrayAccess { .. });
    // `root_index` ends as the key of the access made directly on the root variable.
    let mut root_index = None;
    let mut root = source;
    while let ExprKind::ArrayAccess { array: inner, index } = &root.kind {
        root_index = Some(index.as_ref());
        root = inner;
    }
    let (ExprKind::Variable(name), Some(root_index)) = (&root.kind, root_index) else {
        return Ok(());
    };
    let Some(root_ty) = env.get(name).cloned() else {
        return Ok(());
    };
    if matches!(&root_ty, PhpType::Array(elem) if matches!(elem.as_ref(), PhpType::Never)) {
        // The reference creates the first element of an empty literal: key the container like a
        // keyed write would.
        let key_ty = checker.infer_type(root_index, env)?;
        let widened = match crate::types::normalized_array_key_type(root_index, key_ty) {
            PhpType::Int => PhpType::Array(Box::new(PhpType::Mixed)),
            key @ (PhpType::Str | PhpType::Mixed) => PhpType::AssocArray {
                key: Box::new(key),
                value: Box::new(PhpType::Mixed),
            },
            _ => return Ok(()),
        };
        env.insert(name.clone(), widened);
        return Ok(());
    }
    if nested {
        // `&$maps[0][$key]` over a list of WeakMaps: the last level is an object place, so the
        // container keeps its object element type instead of becoming boxed Mixed values.
        if matches!(
            checker.infer_type(array, env),
            Ok(PhpType::Object(ref class)) if class.eq_ignore_ascii_case("WeakMap")
        ) {
            return Ok(());
        }
        if let Some(widened) = promoted_by_ref_foreach_root_type(&root_ty) {
            env.insert(name.clone(), widened);
        }
    }
    Ok(())
}

/// Restores a narrowed variable in the environment to its previously saved type after a guarded
/// branch, removing it when it had no prior type. Used to keep `if`/`else` type narrowing scoped
/// to its branch.
fn restore_narrowed_var(env: &mut TypeEnv, var: &str, saved: &Option<PhpType>) {
    match saved {
        Some(ty) => {
            env.insert(var.to_string(), ty.clone());
        }
        None => {
            env.remove(var);
        }
    }
}

/// Intersects callable-array facts from every reachable branch of a control-flow join.
///
/// Instance-method arrays additionally require one shared assignment identity across all exits:
/// lowering captures each assignment's receiver in a hidden slot, so equal source expressions in
/// separate branches do not prove equal runtime receivers. Static targets need no capture and
/// remain usable whenever every reachable branch resolves to the same method.
fn joined_callable_array_targets(
    exits: &[HashMap<String, CallableTarget>],
    versions: &[HashMap<String, u64>],
) -> (HashMap<String, CallableTarget>, HashMap<String, u64>) {
    let Some(first) = exits.first() else {
        return (HashMap::default(), HashMap::default());
    };
    let targets = first
        .iter()
        .filter(|(name, target)| {
            exits
                .iter()
                .skip(1)
                .all(|exit| exit.get(name.as_str()) == Some(*target))
        })
        .filter(|(name, target)| {
            if !matches!(target, CallableTarget::Method { .. }) {
                return true;
            }
            let Some(first_version) = versions.first().and_then(|exit| exit.get(name.as_str()))
            else {
                return false;
            };
            versions
                .iter()
                .skip(1)
                .all(|exit| exit.get(name.as_str()) == Some(first_version))
        })
        .map(|(name, target)| (name.clone(), target.clone()))
        .collect::<HashMap<_, _>>();
    let retained_versions = targets
        .keys()
        .filter_map(|name| {
            versions
                .first()
                .and_then(|exit| exit.get(name))
                .map(|version| (name.clone(), *version))
        })
        .collect();
    (targets, retained_versions)
}

/// Names a `foreach` source that PHP accepts but can never iterate, or `None` when the type
/// has no PHP-visible spelling and must stay a hard compile error.
///
/// The names match what php-src prints in `foreach() argument must be of type array|object,
/// <name> given` (captured from PHP 8.5.6): `int`, `float`, `string`, `null`, `resource`, and
/// `true`/`false` for booleans. Only `PhpType::False` pins the boolean value at compile time;
/// a general `bool` is reported as `bool` here, while the RUNTIME warning emitted by
/// `__rt_warn_foreach_non_iterable` always prints the real `true`/`false`.
fn non_iterable_foreach_argument_name(ty: &PhpType) -> Option<&'static str> {
    match ty {
        PhpType::Int => Some("int"),
        PhpType::Float => Some("float"),
        PhpType::Str => Some("string"),
        PhpType::False => Some("false"),
        PhpType::Bool => Some("bool"),
        PhpType::Void => Some("null"),
        PhpType::Resource(_) => Some("resource"),
        _ => None,
    }
}

/// Returns the synthetic constructor default flags for filesystem iterators.
fn filesystem_iterator_default_flags(class_name: &str) -> Option<i64> {
    match class_name {
        "FilesystemIterator" => Some(FS_SKIP_DOTS),
        "GlobIterator" | "RecursiveDirectoryIterator" => Some(0),
        _ => None,
    }
}

impl Checker {
    /// Validates control-flow statements and updates the type environment for their assignment effects.
    ///
    /// Dispatches to specific handlers for `foreach`, `switch`, `if`, `do-while`, `while`, `for`,
    /// `throw`, and `try` constructs. Each handler infers expression types, binds loop/scoped
    /// variables to their PHP-determined types, tracks `break`/`continue` depth, and accumulates
    /// errors for malformed or incompatible constructs. Returns `Ok(())` only when all checks pass.
    pub(crate) fn check_control_flow_stmt(
        &mut self,
        stmt: &crate::parser::ast::Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            } => {
                // A later iteration can observe a reference rebinding performed by an earlier
                // one. Require the body to establish any affirmative managed-cell provenance it
                // consumes within the loop itself.
                self.boxed_ref_aliased_locals.clear();
                self.invalidate_local_flow_narrowing(value_var, env);
                if let Some(key) = key_var {
                    self.invalidate_local_flow_narrowing(key, env);
                }
                // `foreach ($arr as &$v)` takes a reference into each element, so BOTH names it
                // touches are reference-aliased for the rest of the body and neither binding can
                // be killed or re-bound — releasing or abandoning that storage would strand the
                // element references.
                //
                // The ITERABLE is aliased because the loop holds references into its elements.
                // `$v` is aliased because it IS one of those references: PHP leaves it bound to
                // the last element after the loop ends, so a post-loop `$v = "changed"` writes
                // through into `$arr`. The conditional-depth rule does NOT cover it — a `$v`
                // already assigned at depth 0 ABOVE the loop keeps that depth-0 binding, so
                // `local_binding_is_killable` answered true and the permissive path re-bound a
                // name lowering had ref-bound (`mark_ref_bound_local` in
                // `crate::ir_lower::stmt::typed_foreach`). Lowering then refuses the re-bind and
                // degrades to a store through the ref cell at the CELL's type, which for
                // `int` cell + `string` value has no coercion at all: a program the strict
                // checker rejects became one that miscompiles. Marking `$v` here is the same
                // permanent marking a `=&` target receives, and restores the hard error.
                //
                // The source container is aliased before inference, so an iterable that fails to
                // type is still treated as aliased. Capture the displaced value after source
                // effects and before this statement creates its new alias.
                //
                // Both names are REF-BOUND rather than merely lent: `$v` is a cell
                // `typed_foreach` keeps alive across the whole loop (and past it, as PHP
                // leaves it bound to the last element), and the iterable holds the elements
                // those cells point into. Neither slot may be re-represented by a widening.
                if *value_by_ref {
                    self.record_ref_bound_alias_root(array);
                }
                let value_was_bound = env.contains_key(value_var);
                let arr_ty = self.infer_type_with_assignment_effects(array, env);
                let retired_value = self.binding_retirement_type(value_var, env);
                if *value_by_ref {
                    // Retain the permanent alias even when source inference reports an error.
                    self.ref_aliased_locals.insert(value_var.clone());
                    self.non_builtin_ref_aliased_locals
                        .insert(value_var.clone());
                    self.ref_bound_locals.insert(value_var.clone());
                    // Same rule as `=&`: a by-reference foreach binds the element cell,
                    // which is not the boxed capture cell the exemption is about.
                    self.by_ref_capture_boxed_locals.remove(value_var);
                }
                let arr_ty = arr_ty?;
                let warning_site = (self.current_loop_storage_scope.clone(), stmt.span);
                self.foreach_warnings.remove(&warning_site);
                self.record_foreach_entry_exception_flow(stmt, &arr_ty, env);
                if *value_by_ref {
                    widen_by_ref_foreach_source_storage(self, array, env)?;
                }
                if let PhpType::Array(elem_ty) = &arr_ty {
                    // A genuinely packed array has int keys; an UNKNOWN-element array (an
                    // `array`-hinted param/property, elements known only to phpdoc) may be
                    // associative at runtime, so its keys are Mixed (ward-http's
                    // `foreach ($headers as $name => $values)` with string keys).
                    let key_ty = if matches!(elem_ty.as_ref(), PhpType::Mixed) {
                        PhpType::Mixed
                    } else {
                        PhpType::Int
                    };
                    // An indexed foreach-by-reference over a simple local promotes the payload
                    // to hash storage with boxed Mixed entries. Flow typing below the conversion
                    // selects associative operations; earlier expressions were already checked
                    // against the original indexed representation.
                    let promotes_to_hash =
                        *value_by_ref && matches!(&array.kind, ExprKind::Variable(_));
                    // A fresh by-reference binding is null when an empty source never enters the
                    // body, and later writes through the live reference may change its PHP type.
                    // Preserve an existing binding's stricter write-through contract, but give a
                    // fresh binding the Mixed shape used by the runtime reference slot.
                    let value_ty = if *value_by_ref && !value_was_bound {
                        PhpType::Mixed
                    } else {
                        *elem_ty.clone()
                    };
                    self.bind_foreach_iteration(array, value_var, value_ty.clone(),
                        key_var.as_deref().map(|name| (name, key_ty)), retired_value, env);
                    self.update_foreach_callable_metadata(value_var, array, &value_ty);
                    if let Some(k) = key_var { self.clear_foreach_callable_metadata(k); }
                    if promotes_to_hash {
                        if let ExprKind::Variable(source_name) = &array.kind {
                            env.insert(
                                source_name.clone(),
                                PhpType::AssocArray {
                                    key: Box::new(PhpType::Int),
                                    value: Box::new(PhpType::Mixed),
                                },
                            );
                        }
                    }
                } else if let PhpType::AssocArray { key, value } = &arr_ty {
                    // Associative foreach-by-reference converts every entry to boxed Mixed
                    // storage. Reflect that in a simple source local so post-loop reads do not
                    // reinterpret boxed-cell pointers as the old concrete payload type. The
                    // reference local itself retains the entry's declared type, matching the
                    // checker's normal strict write-through rules.
                    let value_ty = if *value_by_ref && !value_was_bound {
                        PhpType::Mixed
                    } else {
                        *value.clone()
                    };
                    self.bind_foreach_iteration(array, value_var, value_ty.clone(),
                        key_var.as_deref().map(|name| (name, *key.clone())), retired_value, env);
                    self.update_foreach_callable_metadata(value_var, array, &value_ty);
                    if let Some(k) = key_var { self.clear_foreach_callable_metadata(k); }
                    if *value_by_ref {
                        if let ExprKind::Variable(source_name) = &array.kind {
                            env.insert(
                                source_name.clone(),
                                PhpType::AssocArray {
                                    key: key.clone(),
                                    value: Box::new(PhpType::Mixed),
                                },
                            );
                        }
                    }
                } else if let PhpType::Object(class_name) = &arr_ty {
                    let is_iter = self.class_implements_interface(class_name, "Iterator")
                        || self.interface_extends_interface(class_name, "Iterator");
                    let is_iter_agg = self
                        .class_implements_interface(class_name, "IteratorAggregate")
                        || self.interface_extends_interface(class_name, "IteratorAggregate");
                    let is_traversable_marker = class_name
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case("Traversable")
                        || self.interface_extends_interface(class_name, "Traversable");
                    // An instantiation inferred THIS round is not in the class table yet; the
                    // next monomorphization round checks the loop against the spliced class.
                    let pending = self.awaits_generic_instantiation(class_name);
                    let (key_ty, value_ty) = if !pending
                        && !is_iter
                        && !is_iter_agg
                        && !is_traversable_marker
                        && !self.object_type_implements_iterable(class_name)
                    {
                        // A by-value walk is the scope-filtered property projection
                        // (`get_object_vars()` from the loop's scope), so it works anywhere: outside
                        // the class only the public properties are visited, as in php. A
                        // by-reference walk binds property slots in place, which only the declaring
                        // scope supports.
                        if *value_by_ref
                            && crate::types::checker::builtin_stdclass::is_stdclass(class_name)
                        {
                            if let Some(walk) = crate::stdclass_by_ref_walk::rewrite(
                                array,
                                key_var.as_deref(),
                                value_var,
                                body,
                                stmt.span,
                            ) {
                                self.stdclass_by_ref_walk_sites.insert(stmt.span);
                                return self.check_stmt(&walk, env);
                            }
                        }
                        if *value_by_ref
                            && !self.current_class.as_deref().is_some_and(|current| {
                                crate::names::php_symbol_key(current)
                                    == crate::names::php_symbol_key(class_name)
                            })
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "foreach over object requires {} to implement Iterator or IteratorAggregate outside its declaring scope",
                                    class_name
                                ),
                            ));
                        }
                        (PhpType::Str, PhpType::Mixed)
                    } else {
                        self.foreach_object_key_value_types(class_name, array)
                    };
                    self.bind_foreach_iteration(array, value_var, value_ty,
                        key_var.as_deref().map(|name| (name, key_ty)), retired_value, env);
                    self.clear_foreach_callable_metadata(value_var);
                    if let Some(k) = key_var { self.clear_foreach_callable_metadata(k); }
                } else if matches!(
                    arr_ty,
                    PhpType::Iterable | PhpType::Mixed | PhpType::Union(_) | PhpType::Never
                ) {
                    // `Never` is a source that cannot produce a value (`foreach ((throw $e) as ...)`):
                    // the loop body is dead, but it is still checked like any other walk.
                    self.bind_foreach_iteration(array, value_var, PhpType::Mixed,
                        key_var.as_deref().map(|name| (name, PhpType::Mixed)), retired_value, env);
                    self.clear_foreach_callable_metadata(value_var);
                    if let Some(k) = key_var { self.clear_foreach_callable_metadata(k); }
                } else if let Some(type_name) = non_iterable_foreach_argument_name(&arr_ty) {
                    // php-src does NOT reject this: `ZEND_FE_RESET_R` raises
                    // `foreach() argument must be of type array|object, <type> given`
                    // (E_WARNING), skips the loop body, and execution continues. Mirroring
                    // that as a hard error would make elephc reject a program PHP runs, so
                    // the diagnostic is a compile warning and codegen emits the same
                    // runtime warning (`IteratorSourceKind::NonIterable` in
                    // `src/codegen/lower_inst/iterators.rs`). Compiler-internal types
                    // (`Packed`, `Pointer`, `Buffer`, `Never`, `Callable`, `TaggedScalar`)
                    // have no PHP-visible spelling and stay a hard error below.
                    self.foreach_warnings.insert(
                        warning_site,
                        (self.current_function.clone(), crate::errors::CompileWarning::new(
                            stmt.span,
                            &format!(
                                "foreach() argument must be of type array|object, {} given; the loop body will never run",
                                type_name
                            ),
                        )),
                    );
                    // The body is still type-checked, so bind both loop variables the way
                    // the `Mixed` source path does.
                    self.bind_foreach_iteration(array, value_var, PhpType::Mixed,
                        key_var.as_deref().map(|name| (name, PhpType::Mixed)), retired_value, env);
                    self.clear_foreach_callable_metadata(value_var);
                } else {
                    return Err(CompileError::new(
                        stmt.span,
                        "foreach requires an array, iterable, or an object implementing Iterator/IteratorAggregate",
                    ));
                }
                // A foreach key is a boxed `Mixed` cell at runtime regardless of
                // the source array's key type, so record the bound name so that a
                // `$dst[$k] = $v` write under it defers to `Op::ArraySetMixedKey`
                // instead of promoting the destination to `AssocArray`.
                if let Some(k) = key_var {
                    self.foreach_key_locals.insert(k.clone());
                }
                if *value_by_ref && matches!(arr_ty, PhpType::Object(_)) {
                    return Err(CompileError::new(
                        stmt.span,
                        "by-reference foreach over Iterator/IteratorAggregate objects is not supported; use an array source or remove &",
                    ));
                }
                // A by-REFERENCE value variable is a cell INTO the iterated array, so every
                // assignment to it is an element write and has to reach the storage fixed point
                // as one. Only a plain local source qualifies: the contract re-types a NAME, and
                // `$this->items` has no name in this environment to re-type.
                let ref_value_binding = match (*value_by_ref, &array.kind) {
                    (true, ExprKind::Variable(source)) => {
                        Some((source.as_str(), value_var.as_str()))
                    }
                    _ => None,
                };
                // Widen after the key/value bindings are in the environment so a push of
                // the foreach value variable joins with its real element type.
                stabilize_loop_storage(self, stmt.span, body, None, None, ref_value_binding, env, None);
                if !*value_by_ref {
                    self.prove_foreach_value_binding(value_var, env);
                }
                // The contract may have just widened the SOURCE's payload — `array<string>` ->
                // `array<mixed>` for `foreach ($chunks as &$chunk) { $chunk = new Chunk($chunk); }`
                // — and the value variable is bound to that payload. Re-read it, or the body is
                // checked against the element type the array had BEFORE the preheader conversion
                // EIR lowering is about to emit, and the store into the cell is refused (or, worse
                // under `--strict-locals` lifted, lowered at the narrower representation).
                if let Some((source, value)) = ref_value_binding {
                    let element = match env.get(source).map(PhpType::codegen_repr) {
                        Some(PhpType::Array(element)) => Some(*element),
                        Some(PhpType::AssocArray { value, .. }) => Some(*value),
                        _ => None,
                    };
                    if let Some(element) = element {
                        env.insert(value.to_string(), element);
                    }
                }
                let errors = self.check_break_continue_target_body(stmt, body, env);
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.infer_type_with_assignment_effects(subject, env)?;
                let mut errors = Vec::new();
                let branch_entry_boxed_refs = self.boxed_ref_aliased_locals.clone();
                let mut arm_entries = Vec::new();
                for (values, _) in cases {
                    let mut entries = Vec::new();
                    for value in values {
                        self.infer_type_with_assignment_effects(value, env)?;
                        self.record_switch_comparison_exception_flow(subject, value, env);
                        entries.push(self.snapshot_flow_exit(env));
                    }
                    // Only this label's evaluated prefix reaches its matching arm. Later
                    // labels still contribute physical storage requirements, not read proofs.
                    arm_entries.push(self.join_flow_snapshots(&entries)
                        .unwrap_or_else(|| self.snapshot_flow_exit(env)));
                }
                let entry = self.snapshot_flow_exit(env);
                let flow = self.snapshot_all_locals_flow(env);
                self.begin_switch_flow(flow, default.is_none());
                let mut arms: Vec<&[Stmt]> = cases.iter().map(|(_, body)| body.as_slice()).collect();
                if let Some(body) = default {
                    let position = crate::termination::switch_default_position(cases, body);
                    arms.insert(position, body);
                    arm_entries.insert(position, entry.clone());
                }
                let last_arm = arms.len().saturating_sub(1);
                let mut fallthrough = None;
                let mut strict_arm_bindings = super::switch_storage::entry_bindings(self, env);
                for (index, (body, direct)) in arms.into_iter().zip(arm_entries).enumerate() {
                    let mut inputs = vec![direct];
                    inputs.extend(fallthrough.take());
                    let joined = self.join_flow_snapshots(&inputs).expect("switch entry");
                    self.restore_flow_snapshot(&joined, env);
                    self.restore_switch_assignment_storage(env);
                    let invalidated = self.conditional_boxed_ref_invalidations.last()
                        .cloned().unwrap_or_default();
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs.iter()
                        .filter(|name| !invalidated.contains(*name)).cloned().collect();
                    let (arm_errors, falls_through) = self.check_body_flow(body, env);
                    let arm_valid = arm_errors.is_empty();
                    errors.extend(arm_errors);
                    if arm_valid {
                        errors.extend(super::switch_storage::observe_arm(
                            self, &mut strict_arm_bindings, body, env, stmt.span));
                    }
                    fallthrough = falls_through.then(|| self.snapshot_flow_exit(env));
                    self.record_switch_case_end(env, index == last_arm && falls_through);
                }
                let frame = self.pop_loop_flow();
                self.publish_loop_flow(frame, env);
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                let storage_entry = self.guarded_union_types.clone();
                let mut flow = self.snapshot_conditional_flow(env);
                self.exclude_conditional_binding_entry(&mut flow);
                // With an `else` every path leaves through one of the arms, so the entry view is
                // not an alternative of the join: `$c = "a" . $i; if (..) { $c = new S; } else {
                // $c = new S($c); } $c->p[] = 1;` reads an `S`, not the `string|S` the entry
                // contributed (symfony/var-dumper `ExceptionCaster::extractSource()`).
                if else_body.is_some() {
                    self.exclude_target_entry_exit(&mut flow);
                }
                let mut errors = Vec::new();

                // Flow-sensitive type narrowing across the if / elseif* / else chain.
                //
                // Each recognized guard narrows its variable to the guarded type while checking
                // that branch's body. The fallthrough env for the remaining clauses (and the final
                // else) accumulates the complement, which is sound because reaching a later clause
                // means every earlier condition was false.
                //
                // After the whole construct we restore every variable we narrowed, so code after
                // the `if` sees the joined view. The single exception is an exhaustively divergent
                // chain (no else and *every* clause body diverges): there the only way to fall
                // through is with all conditions false, so the accumulated complement is sound for
                // the statements after the `if`.
                let mut clauses: Vec<(&Expr, &Vec<Stmt>)> = vec![(condition, then_body)];
                clauses.extend(elseif_clauses.iter().map(|(c, b)| (c, b)));

                // Pre-`if` type of every variable we narrow, captured the first time we touch it,
                // so each one can be restored after the construct.
                let mut saved_vars: Vec<(String, Option<PhpType>)> = Vec::new();
                // The same pre-`if` types, held for `merge_local_assignment_type` so a store
                // inside a guarded region is measured against the BINDING rather than against
                // the guard's view of the value (issue #509). Opened where `saved_vars` records
                // a variable and closed once for the whole construct: the complement a guarded
                // clause publishes narrows the later clauses and the `else` body too, so the
                // origin has to outlive the then-branch it was opened for.
                let mut saved_origins: Vec<SavedNarrowingOrigin> = Vec::new();
                let mut applied_any_guard = false;
                // Single-clause join state: the guarded key and the type it has where the
                // then-branch falls out of the construct. `None` means "no usable fact" (the
                // branch diverged, or a call inside it purged the narrowing).
                let mut join_key: Option<String> = None;
                let mut then_exit_ty: Option<PhpType> = None;
                let single_clause = clauses.len() == 1;
                let mut callable_target_exits = Vec::new();
                let mut callable_target_version_exits = Vec::new();
                // Bindings an `unset()` removed on some branch: the join cannot call them bound.
                let mut unbound_by_branch: Vec<String> = Vec::new();

                for (cond, body) in &clauses {
                    self.infer_type_with_assignment_effects(cond, env)?;
                    let branch_entry_targets = self.callable_array_targets.clone();
                    let branch_entry_target_versions =
                        self.callable_array_target_versions.clone();
                    let branch_entry_boxed_refs = self.boxed_ref_aliased_locals.clone();
                    let branch_entry_reads = self.snapshot_conditional_flow(env);
                    let branch_entry_bound = self.definitely_bound_locals.clone();

                    let guards = self.guard_narrowings(cond, env)?;
                    // `isset($never)` binds `$never` for the true branch only. Narrowing it would
                    // publish the false edge's `null` as a binding after the `if`, and PHP's
                    // `isset()` creates nothing.
                    let isset_roots = Self::truthy_isset_fact_roots(cond, env);
                    let guards: Vec<_> = guards
                        .into_iter()
                        .filter(|guard| !isset_roots.contains(&guard.var))
                        .collect();
                    applied_any_guard |= !guards.is_empty();
                    let saved: Vec<_> = guards.iter().map(|guard| {
                        (guard.var.clone(), env.get(&guard.var).cloned())
                    }).collect();
                    for guard in &guards {
                        self.box_iterable_local_narrowed_to_array(guard, env);
                        if !saved_vars.iter().any(|(v, _)| v == &guard.var) {
                            saved_vars.push((guard.var.clone(), env.get(&guard.var).cloned()));
                        }
                        saved_origins.push(self.enter_flow_narrowing(
                            &guard.var, env.get(&guard.var), &guard.then_ty,
                        ));
                        env.insert(guard.var.clone(), guard.then_ty.clone());
                    }
                    // A probe root the condition proved set is a fact of this branch only: bound
                    // `Mixed` for the body, dropped again before the branch exit is joined.
                    let installed_isset_roots: Vec<String> = isset_roots
                        .iter()
                        .filter(|root| !env.contains_key(*root))
                        .cloned()
                        .collect();
                    for root in &installed_isset_roots {
                        env.insert(root.clone(), PhpType::Mixed);
                    }
                    errors.extend(self.check_body(body, env));
                    for root in &installed_isset_roots {
                        if env.get(root) == Some(&PhpType::Mixed) {
                            env.remove(root);
                        }
                    }
                    if !crate::termination::block_guarantees_function_exit_with_divergence(
                        body, &|expr| self.expr_is_declared_never_call(expr),
                    ) {
                        if self.body_cannot_fall_through(body) {
                            self.observe_terminal_flow_storage(&mut flow, env);
                        } else {
                            self.observe_conditional_flow_exit(&mut flow, env);
                        }
                    }
                    // Preserve lazy-property joins and nullable locals initialized on their null arm.
                    if single_clause && guards.len() == 1 {
                        let guard = &guards[0];
                        let branch_exit = env.get(&guard.var);
                        let nullable_initialized_local = !Self::narrowed_place_key_is_property(&guard.var)
                            && branch_exit.is_some_and(|ty|
                                (*ty == PhpType::Int || (matches!(ty, PhpType::Object(_))
                                    && self.local_flow_read_is_stable(&guard.var)))
                                && saved.iter().any(|(name, original)| name == &guard.var
                                    && matches!(original, Some(PhpType::Union(members)) if members.len() == 2
                                        && members.contains(&PhpType::Void)
                                        && members.iter().any(|member|
                                            matches!((member, ty), (PhpType::Int, PhpType::Int)
                                                | (PhpType::Object(_), PhpType::Object(_)))
                                                && self.type_contains_values(member, ty)))));
                        if (Self::narrowed_place_key_is_property(&guard.var) || nullable_initialized_local)
                            && !self.body_cannot_fall_through(body)
                            && branch_exit.is_some_and(|ty| *ty != guard.then_ty)
                        {
                            join_key = Some(guard.var.clone());
                            then_exit_ty = branch_exit.cloned();
                        }
                    }
                    if !self.body_cannot_fall_through(body) {
                        callable_target_exits.push(self.callable_array_targets.clone());
                        callable_target_version_exits
                            .push(self.callable_array_target_versions.clone());
                    }
                    self.callable_array_targets = branch_entry_targets;
                    self.callable_array_target_versions = branch_entry_target_versions;
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs;
                    self.restore_statement_flow_storage(&flow, &branch_entry_reads);
                    self.restore_conditional_read_inputs(&branch_entry_reads, env);
                    if !self.body_cannot_fall_through(body) {
                        unbound_by_branch.extend(
                            branch_entry_bound.iter()
                                .filter(|name| !self.definitely_bound_locals.contains(*name))
                                .cloned(),
                        );
                    }
                    self.definitely_bound_locals = branch_entry_bound;
                    for (var, previous) in &saved {
                        restore_narrowed_var(env, var, previous);
                    }
                    // Reaching a later clause means this entire compound condition was false.
                    for guard in &guards {
                        env.insert(guard.var.clone(), guard.else_ty.clone());
                        self.republish_flow_narrowing(&guard.var, &guard.else_ty);
                    }
                }

                // Final else body (if present) is checked with the accumulated complement.
                // `None` = the else path cannot reach the code after the `if`. `Some(None)` = it
                // can, but the guarded fact was lost there. `Some(Some(ty))` = it can and the
                // fact is `ty`.
                let mut else_exit_ty: Option<Option<PhpType>> = None;
                let mut else_falls_through = else_body.is_none();
                if let Some(body) = else_body {
                    let else_entry_bound = self.definitely_bound_locals.clone();
                    errors.extend(self.check_body(body, env));
                    else_falls_through = !self.body_cannot_fall_through(body);
                    if else_falls_through {
                        unbound_by_branch.extend(
                            else_entry_bound.iter()
                                .filter(|name| !self.definitely_bound_locals.contains(*name))
                                .cloned(),
                        );
                    }
                    if !crate::termination::block_guarantees_function_exit_with_divergence(
                        body, &|expr| self.expr_is_declared_never_call(expr),
                    ) {
                        if self.body_cannot_fall_through(body) {
                            self.observe_terminal_flow_storage(&mut flow, env);
                        } else {
                            self.observe_conditional_flow_exit(&mut flow, env);
                        }
                    }
                }
                if else_falls_through && else_body.is_none() {
                    self.observe_conditional_binding_exit(&mut flow);
                }
                for name in &unbound_by_branch {
                    self.definitely_bound_locals.remove(name);
                }
                if else_falls_through {
                    callable_target_exits.push(self.callable_array_targets.clone());
                    callable_target_version_exits
                        .push(self.callable_array_target_versions.clone());
                }
                let (joined_targets, joined_target_versions) = joined_callable_array_targets(
                    &callable_target_exits,
                    &callable_target_version_exits,
                );
                self.callable_array_targets = joined_targets;
                self.callable_array_target_versions = joined_target_versions;
                if let Some(key) = &join_key {
                    if else_falls_through {
                        else_exit_ty = Some(env.get(key).cloned());
                    }
                }

                // Keep the accumulated complement for the statements after the `if` only when no
                // guarded clause can fall through: there is no else and every clause ends in a
                // non-fallthrough statement, so reaching the following code implies all conditions
                // were false. Otherwise restore every narrowed variable to its pre-`if` type.
                let keep_complement_after_if = applied_any_guard
                    && else_body.is_none()
                    && clauses
                        .iter()
                        .all(|(_, body)| self.body_cannot_fall_through(body));
                // A single guarded clause whose then-branch also falls through joins the two
                // exit facts instead of discarding both. `if (X === null) { X = new S(); }`
                // leaves `S` on the then path (the write recorded it) and `S` on the else path
                // (the guard complement), so the union is `S` — the singleton pattern.
                let joined = join_key.as_ref().and_then(|key| {
                    let then_ty = then_exit_ty.clone()?;
                    let joined = match &else_exit_ty {
                        None => then_ty,
                        Some(Some(else_ty)) if matches!((&then_ty, else_ty),
                            (PhpType::Object(_), PhpType::Object(_))) =>
                        {
                            self.join_flow_views(&then_ty, else_ty)
                        }
                        Some(Some(else_ty)) => {
                            self.normalize_union_type(vec![then_ty, else_ty.clone()])
                        }
                        Some(None) => return None,
                    };
                    Some((key.clone(), joined))
                });
                let joined_local_key = joined.as_ref().filter(|(key, ty)|
                    !Self::narrowed_place_key_is_property(key)
                        && matches!(ty, PhpType::Int | PhpType::Object(_)))
                    .map(|(key, _)| key.clone());
                if !keep_complement_after_if {
                    self.restore_guarded_storage_provenance(&storage_entry);
                    for (var, original) in &saved_vars {
                        restore_narrowed_var(env, var, original);
                    }
                    if let Some((key, joined)) = joined {
                        env.insert(key, joined);
                    }
                } else {
                    for (var, original) in &saved_vars {
                        if let Some(original) = original {
                            if !Self::narrowed_place_key_is_property(var) {
                                self.guarded_union_types.entry(var.clone())
                                    .and_modify(|entry| entry.coalesced = false)
                                    .or_insert(super::super::GuardedLocalStorage { ty: original.clone(), coalesced: false });
                            }
                        }
                    }
                }
                // Closed for the construct even when the complement is KEPT for the code after
                // it. A kept complement is a sound fact about every path that gets there, so the
                // statements after the `if` are ordinary straight-line code: a store that does
                // not fit takes the depth-0 re-bind path, which abandons the slot properly,
                // rather than being waved through against a binding the `if` may have replaced.
                for saved_origin in saved_origins.into_iter().rev() {
                    self.exit_flow_narrowing(saved_origin);
                }
                if keep_complement_after_if {
                    self.keep_conditional_views(&mut flow, env, saved_vars.into_iter().map(|(name, _)| name));
                }
                if let Some(key) = joined_local_key {
                    self.keep_conditional_views(&mut flow, env, std::iter::once(key));
                }
                self.finish_statement_conditional_flow(flow, env);

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::DoWhile { body, condition } => {
                self.boxed_ref_aliased_locals.clear();
                stabilize_loop_storage(self, stmt.span, body, None, Some(condition), None, env, None);
                let flow = self.snapshot_loop_flow(env);
                self.begin_loop_flow(flow, false, !crate::termination::expr_is_truthy_literal(condition));
                let mut errors = self.check_loop_body(body, env);
                let backedge = self.loop_backedge_env();
                let mut tail_env = backedge.clone().unwrap_or_else(|| env.clone());
                if let Err(error) = self.infer_type_with_assignment_effects(condition, &mut tail_env) {
                    errors.extend(error.flatten());
                }
                self.record_loop_tail_end(&tail_env, backedge.is_some());
                if backedge.is_some() { *env = tail_env; }
                let frame = self.pop_loop_flow();
                self.publish_loop_flow(frame, env);
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::While { condition, body } => {
                self.boxed_ref_aliased_locals.clear();
                stabilize_loop_storage(self, stmt.span, body, None, Some(condition), None, env, None);
                self.infer_type_with_assignment_effects(condition, env)?;
                let flow = self.snapshot_loop_flow(env);
                // The condition is re-evaluated before every iteration, so a guard on it
                // holds on entry to each one: `while (($row = fgetcsv($h)) !== false)`
                // leaves `$row` an array inside the body. The narrowing is dropped again
                // afterwards, because the loop exits precisely when the guard is false.
                let guards = self.guard_narrowings(condition, env)?;
                let saved: Vec<_> = guards.iter().map(|guard| {
                    (guard.var.clone(), env.get(&guard.var).cloned())
                }).collect();
                let mut saved_origins = Vec::new();
                for guard in &guards {
                    saved_origins.push(self.enter_flow_narrowing(
                        &guard.var, env.get(&guard.var), &guard.then_ty,
                    ));
                    env.insert(guard.var.clone(), guard.then_ty.clone());
                }
                let can_exit = !crate::termination::expr_is_truthy_literal(condition);
                self.begin_loop_flow(flow, can_exit, can_exit);
                let errors = self.check_loop_body(body, env);
                let frame = self.pop_loop_flow();
                for saved_origin in saved_origins.into_iter().rev() {
                    self.exit_flow_narrowing(saved_origin);
                }
                for (var, previous) in &saved {
                    restore_narrowed_var(env, var, previous);
                }
                self.publish_loop_flow(frame, env);
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(s) = init {
                    // The initializer runs exactly once, unconditionally, whenever the `for`
                    // statement itself runs: `for ($i = 0; …)` binds `$i` exactly like `$i = 0;`
                    // written before the loop. It is therefore checked at the depth of the
                    // statement, not at the depth of the conditional group `check_stmt` opened
                    // for the whole loop, so its bindings are unconditional ones (a later
                    // straight-line `unset` or retype may end them) and it may itself retype or
                    // re-bind an unconditional binding. The provenance bracket of that group is
                    // kept: only the binding depth changes. `mixed_storage_scan` mirrors this.
                    let loop_depth = self.local_conditional_depth;
                    self.local_conditional_depth = loop_depth.saturating_sub(1);
                    let checked = self.check_stmt(s, env);
                    self.local_conditional_depth = loop_depth;
                    checked?;
                }
                self.boxed_ref_aliased_locals.clear();
                // A `for ($i = 0; …; $i++)` counter is the one index a still-empty array can be
                // written through without leaving packed storage, because it is 0 at the first
                // write and grows one slot at a time with the array. Recorded with the depth its
                // body runs at, so a write under an `if` — which could skip a slot — is not
                // covered. Saved and restored so a nested loop's counter cannot outlive it.
                let saved_packed_counter = self.packed_loop_counter.take();
                // A counter a reference already reaches is not provable from the body: `$r = &$i`
                // before the loop lets `$r += 2` inside it advance `$i` with nothing in the body
                // naming `$i`. `ref_aliased_locals` holds every local of this body a reference
                // can reach, permanently, so such a counter takes hash storage.
                self.packed_loop_counter =
                    crate::types::checker::packed_counter::packed_for_counter(
                        init.as_deref(),
                        condition.as_ref(),
                        update.as_deref(),
                        body,
                        self.local_conditional_depth,
                    )
                    .filter(|counter| !self.ref_aliased_locals.contains(&counter.name));
                // Cloned first: the stabilizer takes `&mut Checker`, so the name cannot be
                // borrowed out of `self` across the call.
                let packed_counter_name = self
                    .packed_loop_counter
                    .as_ref()
                    .map(|counter| counter.name.clone());
                stabilize_loop_storage(
                    self,
                    stmt.span,
                    body,
                    update.as_deref(),
                    condition.as_ref(),
                    None,
                    env,
                    packed_counter_name.as_deref(),
                );
                super::loop_entry::prepare_for_update_reads(self, update.as_deref(), env);
                // The rest of the arm re-walks this same body, so the counter stays registered
                // through it — the storage decision must not come out differently on the second
                // walk than on the first — and is restored on every exit, error paths included.
                let outcome = self.check_for_tail(condition.as_ref(), update.as_deref(), body, env);
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                self.packed_loop_counter = saved_packed_counter;
                outcome
            }
            StmtKind::Throw(expr) => {
                let thrown_ty = self.infer_type_with_assignment_effects(expr, env)?;
                self.validate_finally_throw_type(&thrown_ty, stmt.span)?;
                self.record_finally_throw(&thrown_ty, expr, env);
                Ok(())
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let mut resolved_catches = Vec::new();
                for catch_clause in catches {
                    let mut resolved_types = Vec::new();
                    for raw_exception_type in &catch_clause.exception_types {
                        let exception_type =
                            self.resolve_catch_type_name(raw_exception_type, stmt.span)?;
                        let catch_type_is_known = self.classes.contains_key(&exception_type)
                            || self.interfaces.contains_key(&exception_type);
                        if catch_type_is_known
                            && !self.object_type_implements_throwable(&exception_type)
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Catch type must extend or implement Throwable: {}",
                                    exception_type
                                ),
                            ));
                        }
                        if !catch_type_is_known {
                            self.unresolved_catch_types.insert(exception_type.clone());
                        }
                        // PHP permits unresolved catch names: if the class never loads, that arm
                        // simply cannot match. Keep the declared nominal type so unreachable-arm
                        // analysis does not erase callback and collection element precision.
                        resolved_types.push(exception_type);
                    }
                    resolved_catches.push(resolved_types);
                }
                let mut errors = Vec::new();
                let pending_exits = self.pending_loop_exit_counts();
                let pending_function_exits = self.pending_finally_exit_counts();
                let try_entry = self.snapshot_flow_exit(env);
                self.begin_finally_region(resolved_catches.clone(), finally_body.is_some());
                let branch_entry_boxed_refs = self.boxed_ref_aliased_locals.clone();
                let (try_errors, try_falls_through) = self.check_body_flow(try_body, env);
                errors.extend(try_errors);
                let try_tail = self.snapshot_flow_exit(env);
                let mut normal_exits = Vec::new();
                if try_falls_through {
                    normal_exits.push(self.snapshot_flow_exit(env));
                }
                self.enter_finally_catches();
                for (index, (catch_clause, resolved_types)) in catches.iter().zip(resolved_catches).enumerate() {
                    let reachable = self.enter_catch_flow(index, &try_tail, &try_entry, env);
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs.clone();
                    if let Some(variable) = &catch_clause.variable {
                        let retired = self.binding_retirement_type(variable, env);
                        self.definitely_bound_locals.insert(variable.clone());
                        self.record_loop_binding_end(variable);
                        self.invalidate_local_flow_narrowing(variable, env);
                        env.insert(
                            variable.clone(),
                            PhpType::Object(self.common_catch_type_name(&resolved_types)),
                        );
                        if reachable { self.record_binding_retirement_exception_flow(&retired, env); }
                    }
                    let catch_exit_counts = self.pending_finally_exit_counts();
                    let catch_loop_counts = self.pending_loop_exit_counts();
                    let (catch_errors, catch_falls_through) = self.check_body_flow(&catch_clause.body, env);
                    errors.extend(catch_errors);
                    if reachable && catch_falls_through {
                        normal_exits.push(self.snapshot_flow_exit(env));
                    } else if !reachable {
                        self.discard_finally_exits_after(&catch_exit_counts);
                        self.apply_finally_to_loop_exits(&catch_loop_counts, env, true);
                    }
                }
                let falls_through = !normal_exits.is_empty();
                if let Some(joined) = self.join_scoped_flow_snapshots(&normal_exits, &try_entry) {
                    self.restore_flow_snapshot(&joined, env);
                } else { self.restore_flow_snapshot(&try_entry, env); }
                let function_exits = self.end_finally_region();
                if let Some(body) = finally_body {
                    let continuation = falls_through.then(|| self.snapshot_flow_exit(env));
                    self.join_pending_finally_inputs(&pending_exits, &function_exits, falls_through, &try_entry, env);
                    let before_finally_exits = self.pending_finally_exit_counts();
                    let invalidated = self
                        .conditional_boxed_ref_invalidations
                        .last()
                        .cloned()
                        .unwrap_or_default();
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs
                        .into_iter()
                        .filter(|name| !invalidated.contains(name))
                        .collect();
                    self.finally_break_continue_bases
                        .push(self.break_continue_depth);
                    errors.extend(self.check_body(body, env));
                    self.finally_break_continue_bases.pop();
                    let diverges = crate::termination::block_guarantees_function_exit_with_divergence(
                        body, &|expr| self.expr_is_declared_never_call(expr));
                    self.apply_finally_to_loop_exits(&pending_exits, env, diverges);
                    self.apply_finally_to_function_exits(&pending_function_exits, &before_finally_exits, env, diverges);
                    if !diverges {
                        if let Some(continuation) = continuation {
                            self.restore_finally_continuation_reads(&continuation, body, env);
                        }
                    }
                }
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            _ => unreachable!("non-control-flow statement routed to control-flow checker"),
        }
    }

    /// Gives an `iterable` local that a guard narrows to an array boxed `Mixed` storage for its
    /// whole frame, from the incoming-parameter store on.
    ///
    /// `iterable` storage is a bare heap pointer (an array or a `Traversable`). The narrowed reads
    /// still load that pointer, which array builtins cannot take (`implode()` was refused), and a
    /// by-reference one wrote an array back through it as if the slot were one: Symfony's
    /// `if (!is_array($providers)) { $providers = iterator_to_array($providers); }
    /// array_unshift($providers, ...)` produced garbage. A boxed slot takes every builtin's
    /// run-time-checked `Mixed` path instead.
    fn box_iterable_local_narrowed_to_array(&mut self, guard: &super::narrowing::GuardNarrowing, env: &TypeEnv) {
        let narrows_to_array = |ty: &PhpType| {
            matches!(ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
        };
        if env.get(&guard.var).map(PhpType::codegen_repr) != Some(PhpType::Iterable)
            || !(narrows_to_array(&guard.then_ty) || narrows_to_array(&guard.else_ty))
        {
            return;
        }
        self.by_ref_local_storage_types.insert(
            (self.current_loop_storage_scope.clone(), guard.var.clone()),
            PhpType::Mixed,
        );
    }

    /// Returns the static key and value types exposed by foreach over an object iterator.
    ///
    /// Concrete `Iterator` implementations can narrow `key()`/`current()` from the
    /// interface's `mixed` contract, so foreach should expose those narrower types inside
    /// the loop. IteratorAggregate sources are resolved through their `getIterator()`
    /// return type when that type is statically known.
    fn foreach_object_key_value_types(
        &self,
        class_name: &str,
        source: &Expr,
    ) -> (PhpType, PhpType) {
        let value_override = self.foreach_object_value_type_override(class_name, source);
        if self.class_implements_interface(class_name, "Iterator")
            || self.interface_extends_interface(class_name, "Iterator")
        {
            return (
                self.iterator_method_return_type(class_name, "key"),
                value_override.unwrap_or_else(|| {
                    self.iterator_method_return_type(class_name, "current")
                }),
            );
        }

        let get_iterator_ty = self.iterator_method_return_type(class_name, "getIterator");
        if let PhpType::Object(iterator_name) = get_iterator_ty {
            return (
                self.iterator_method_return_type(&iterator_name, "key"),
                value_override.unwrap_or_else(|| {
                    self.iterator_method_return_type(&iterator_name, "current")
                }),
            );
        }

        (PhpType::Mixed, PhpType::Mixed)
    }

    /// Returns a narrower foreach value type for SPL filesystem iterators when flags are static.
    fn foreach_object_value_type_override(
        &self,
        class_name: &str,
        source: &Expr,
    ) -> Option<PhpType> {
        if class_name == "DirectoryIterator" {
            return Some(PhpType::Object("DirectoryIterator".to_string()));
        }
        let flags = self.filesystem_iterator_source_flags(class_name, source)?;
        match flags & FS_CURRENT_MODE_MASK {
            FS_CURRENT_AS_PATHNAME => None,
            FS_CURRENT_AS_SELF => Some(PhpType::Object(class_name.to_string())),
            _ => Some(PhpType::Object("SplFileInfo".to_string())),
        }
    }

    /// Returns constructor flags for statically constructed filesystem iterators.
    fn filesystem_iterator_source_flags(&self, class_name: &str, source: &Expr) -> Option<i64> {
        if !matches!(
            class_name,
            "FilesystemIterator" | "GlobIterator" | "RecursiveDirectoryIterator"
        ) {
            return None;
        }
        let ExprKind::NewObject {
            class_name: source_class,
            args,
        } = &source.kind
        else {
            return None;
        };
        if source_class.as_str() != class_name {
            return None;
        }
        args.get(1)
            .and_then(|expr| self.eval_static_int_expr(expr))
            .or_else(|| filesystem_iterator_default_flags(class_name))
    }

    /// Evaluates a side-effect-free integer expression used for SPL flag constants.
    pub(in crate::types::checker) fn eval_static_int_expr(&self, expr: &Expr) -> Option<i64> {
        match &expr.kind {
            ExprKind::IntLiteral(value) => Some(*value),
            ExprKind::Negate(inner) => self.eval_static_int_expr(inner).map(|value| -value),
            ExprKind::BitNot(inner) => self.eval_static_int_expr(inner).map(|value| !value),
            ExprKind::BinaryOp { left, op, right } => {
                let left = self.eval_static_int_expr(left)?;
                let right = self.eval_static_int_expr(right)?;
                match op {
                    BinOp::BitOr => Some(left | right),
                    BinOp::BitAnd => Some(left & right),
                    BinOp::BitXor => Some(left ^ right),
                    BinOp::Add => Some(left + right),
                    BinOp::Sub => Some(left - right),
                    _ => None,
                }
            }
            ExprKind::ScopedConstantAccess { receiver, name } => {
                self.class_constant_int_value(receiver, name)
            }
            _ => None,
        }
    }

    /// Resolves a class constant integer value from checker metadata.
    fn class_constant_int_value(&self, receiver: &StaticReceiver, name: &str) -> Option<i64> {
        let StaticReceiver::Named(class_name) = receiver else {
            return None;
        };
        self.classes
            .get(class_name.as_str())
            .and_then(|class_info| class_info.constants.get(name))
            .and_then(|expr| self.eval_static_int_expr(expr))
    }

    /// Looks up an iterator-related method return type on either a class or an interface.
    ///
    /// Missing metadata falls back to `mixed`, matching PHP's loose iterator contracts and
    /// preserving the previous conservative behavior for dynamic or unknown iterator shapes.
    fn iterator_method_return_type(&self, type_name: &str, method: &str) -> PhpType {
        let method_key = crate::names::php_symbol_key(method);
        if type_name == "DirectoryIterator" && method_key == "current" {
            return PhpType::Object("DirectoryIterator".to_string());
        }
        if let Some(class_info) = self.classes.get(type_name) {
            return class_info
                .methods
                .get(&method_key)
                .map(|sig| sig.return_type.clone())
                .unwrap_or(PhpType::Mixed);
        }
        self.interfaces
            .get(type_name)
            .and_then(|interface_info| interface_info.methods.get(&method_key))
            .map(|sig| sig.return_type.clone())
            .unwrap_or(PhpType::Mixed)
    }

    /// Checks the condition, body, and update in PHP execution order.
    fn check_for_tail(
        &mut self, condition: Option<&Expr>, update: Option<&Stmt>,
        body: &[Stmt], env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        if let Some(condition) = condition {
            self.infer_type_with_assignment_effects(condition, env)?;
        }
        // Each clause can define what the other reads: the update runs after the body, on the
        // back edge, so nette's `Process` writing `$bytes` in the body of
        // `for ($written = 0; $written < $length; $written += $bytes)` reads it there.
        let flow = self.snapshot_loop_flow(env);
        let can_exit = condition.is_some_and(|condition| !crate::termination::expr_is_truthy_literal(condition));
        self.begin_loop_flow(flow, can_exit, can_exit);
        let mut errors = self.check_loop_body(body, env);
        let backedge = self.loop_backedge_env();
        let mut tail_env = backedge.clone().unwrap_or_else(|| env.clone());
        if let Some(update) = update {
            if let Err(error) = self.check_stmt(update, &mut tail_env) {
                errors.extend(error.flatten());
            }
        }
        self.record_loop_tail_end(&tail_env, backedge.is_some());
        if backedge.is_some() { *env = tail_env; }
        let frame = self.pop_loop_flow();
        self.publish_loop_flow(frame, env);
        if errors.is_empty() { Ok(()) } else { Err(CompileError::from_many(errors)) }
    }

    /// Checks a foreach body while collecting its distinct loop exits.
    fn check_break_continue_target_body(&mut self, stmt: &Stmt, body: &[Stmt], env: &mut TypeEnv) -> Vec<CompileError> {
        let flow = self.snapshot_loop_flow(env);
        self.begin_loop_flow(flow, true, true);
        if let StmtKind::Foreach { key_var, value_var, .. } = &stmt.kind {
            self.definitely_bound_locals.insert(value_var.clone());
            self.definitely_bound_locals.extend(key_var.iter().cloned());
        }
        let errors = self.check_loop_body(body, env);
        self.record_iteration_exception_flow(stmt);
        let frame = self.pop_loop_flow();
        self.publish_loop_flow(frame, env);
        errors
    }

    /// Saves the normal back edge separately from break and continue snapshots.
    fn check_loop_body(&mut self, body: &[Stmt], env: &mut TypeEnv) -> Vec<CompileError> {
        Self::purge_property_narrowings(env);
        let errors = self.check_body(body, env);
        self.record_loop_body_end(env, !self.body_cannot_fall_through(body));
        errors
    }

    /// Updates callable metadata for a foreach value variable.
    ///
    /// Homogeneous arrays that store callable descriptors keep their signature
    /// and capture metadata under the source array variable name. A foreach value
    /// binding from that array must expose the same metadata to calls emitted in
    /// the loop body.
    fn update_foreach_callable_metadata(
        &mut self,
        dest: &str,
        source_array: &Expr,
        value_ty: &PhpType,
    ) {
        if value_ty != &PhpType::Callable {
            self.clear_foreach_callable_metadata(dest);
            return;
        }
        if let ExprKind::Variable(src_name) = &source_array.kind {
            self.copy_foreach_callable_metadata(dest, src_name);
        } else {
            self.clear_foreach_callable_metadata(dest);
        }
    }

    /// Copies callable signature, capture, first-class target, and callable-array metadata.
    fn copy_foreach_callable_metadata(&mut self, dest: &str, src: &str) {
        let copied_target_version = self.callable_array_target_versions.get(src).copied();
        self.mark_callable_array_target_write(dest);
        if let Some(return_ty) = self.closure_return_types.get(src).cloned() {
            self.closure_return_types.insert(dest.to_string(), return_ty);
        } else {
            self.closure_return_types.remove(dest);
        }
        if let Some(sig) = self.callable_sigs.get(src).cloned() {
            self.callable_sigs.insert(dest.to_string(), sig);
        } else {
            self.callable_sigs.remove(dest);
        }
        if let Some(captures) = self.callable_captures.get(src).cloned() {
            self.callable_captures.insert(dest.to_string(), captures);
        } else {
            self.callable_captures.remove(dest);
        }
        if let Some(target) = self.callable_array_targets.get(src).cloned() {
            self.callable_array_targets
                .insert(dest.to_string(), target);
            if let Some(version) = copied_target_version {
                self.callable_array_target_versions
                    .insert(dest.to_string(), version);
            }
        } else {
            self.callable_array_targets.remove(dest);
        }
        if let Some(target) = self.first_class_callable_targets.get(src).cloned() {
            self.first_class_callable_targets
                .insert(dest.to_string(), target);
        } else {
            self.first_class_callable_targets.remove(dest);
        }
    }

    /// Clears callable metadata for a foreach key or value binding.
    fn clear_foreach_callable_metadata(&mut self, dest: &str) {
        self.record_string_suffix_binding(
            (self.current_loop_storage_scope.clone(), dest.to_string()),
            None,
        );
        self.mark_callable_array_target_write(dest);
        self.closure_return_types.remove(dest);
        self.callable_sigs.remove(dest);
        self.callable_captures.remove(dest);
        self.callable_array_targets.remove(dest);
        self.first_class_callable_targets.remove(dest);
    }

    /// Checks each statement in a body sequentially, collecting all errors.
    ///
    /// Unlike `check_break_continue_target_body`, this does not update `break_continue_depth`.
    /// Used for `switch` cases, `if` branches, `try` blocks, and other bodies where the
    /// break/continue level is managed at a higher level.
    fn check_body(&mut self, body: &[Stmt], env: &mut TypeEnv) -> Vec<CompileError> {
        self.check_body_flow(body, env).0
    }

    /// Retains semantic fallthrough separately from the environment of unreachable suffixes.
    fn check_body_flow(&mut self, body: &[Stmt], env: &mut TypeEnv) -> (Vec<CompileError>, bool) {
        let mut errors = Vec::new();
        let mut reachable = true;
        for stmt in body {
            let selected_body = self.statement_selected_body_at_entry(stmt, env);
            let pending = self.pending_loop_exit_counts();
            let function_exits = self.pending_finally_exit_counts();
            if let Err(error) = self.check_stmt(stmt, env) {
                errors.extend(error.flatten());
            }
            if !reachable {
                self.apply_finally_to_loop_exits(&pending, env, true);
                self.discard_finally_exits_after(&function_exits);
            }
            // A call can resolve its declared `never` signature while the statement is checked.
            let terminal_body = selected_body.unwrap_or_else(|| std::slice::from_ref(stmt));
            reachable &= !self.body_cannot_fall_through(terminal_body);
        }
        (errors, reachable)
    }

    /// Selects a known guard arm before writes change the entry environment.
    fn statement_selected_body_at_entry<'a>(&self, stmt: &'a Stmt, env: &TypeEnv) -> Option<&'a [Stmt]> {
        if let StmtKind::If { condition, then_body, elseif_clauses, else_body } = &stmt.kind {
            for (condition, body) in std::iter::once((condition, then_body)).chain(elseif_clauses.iter().map(|(c, b)| (c, b))) {
                match self.known_null_guard_truth(condition, env) {
                    Some(true) => return Some(body),
                    Some(false) => {},
                    None => return None,
                }
            }
            return Some(else_body.as_deref().unwrap_or(&[]));
        }
        None
    }
}
