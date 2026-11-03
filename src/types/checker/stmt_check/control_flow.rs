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
use std::collections::{HashMap, HashSet};

use crate::parser::ast::{
    BinOp, CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind, Visibility,
};
use crate::termination::{block_terminal_effect_with_divergence, TerminalEffect};
use crate::types::{PhpType, TypeEnv};

use super::super::{Checker, SavedNarrowingOrigin};

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

/// Computes and records fixed-point array storage contracts before checking a loop body.
///
/// The shared analysis iterates over rebinds and growth sites with an evolving environment, so
/// cascading promotions, non-literal RHSs, and raw-to-raw element changes converge before any
/// header/body read is checked. EIR lowering later consumes the recorded contract for the same
/// loop span rather than repeating expression inference.
///
/// `ref_value_binding` is `Some((source, value))` only for a by-REFERENCE `foreach` whose source is
/// a plain local — the one shape whose body writes reach the iterated array without naming it.
fn stabilize_loop_storage(
    checker: &mut Checker,
    loop_span: crate::span::Span,
    body: &[Stmt],
    update: Option<&Stmt>,
    ref_value_binding: Option<(&str, &str)>,
    env: &mut TypeEnv,
    packed_counter: Option<&str>,
) {
    let key = (checker.current_loop_storage_scope.clone(), loop_span);
    if let Some(recorded) = checker.loop_storage_types.get(&key).cloned() {
        for (name, storage_type) in recorded {
            env.insert(name, storage_type);
        }
        return;
    }
    let snapshot = env.clone();
    let mut call_types: std::collections::HashMap<crate::span::Span, PhpType> =
        std::collections::HashMap::new();
    let contracts = crate::types::checker::loop_carried_storage_types(
        body,
        update,
        &snapshot,
        packed_counter,
        ref_value_binding,
        &mut |expr, analysis_env| {
            let is_call = matches!(
                expr.kind,
                ExprKind::FunctionCall { .. }
                    | ExprKind::MethodCall { .. }
                    | ExprKind::StaticMethodCall { .. }
                    | ExprKind::ClosureCall { .. }
                    | ExprKind::ExprCall { .. }
            );
            // The memo is keyed by span, so it may only answer for a span that names one call.
            // Under `Span::dummy()` every method / static / closure call in a prelude loop body
            // shared a single entry and the FIRST call's inferred type was handed to all of
            // them — 42 times while checking one `new PDO("sqlite::memory:")` program.
            let memoizable = is_call && expr.span.identifies_a_node();
            if memoizable {
                if let Some(cached) = call_types.get(&expr.span) {
                    return Some(cached.clone());
                }
            }
            // This body and its lexical closures see an unguarded environment. Independently
            // checked callees still need their facts, including a user function named `main`.
            checker.member_receiver_analysis_scopes.push((
                checker.current_function.clone(), checker.current_loop_storage_scope.clone(),
            ));
            let inferred = checker.infer_type(expr, analysis_env);
            let _ = checker.member_receiver_analysis_scopes.pop();
            let inferred = inferred.ok()?;
            if memoizable {
                call_types.insert(expr.span, inferred.clone());
            }
            Some(inferred)
        },
    );
    let (scope, span) = key;
    if !checker.loop_storage_types.contains_key(&(scope.clone(), span)) {
        checker
            .loop_storage_scope_spans
            .entry(scope.clone())
            .or_default()
            .push(span);
    }
    let recorded = checker.loop_storage_types.entry((scope, span)).or_default();
    for (name, storage_type) in contracts {
        recorded.insert(name.clone(), storage_type.clone());
        env.insert(name, storage_type);
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
        let mut reachable_declarations = HashSet::from([declaring_class]);
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
        return (HashMap::new(), HashMap::new());
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

/// Joins the bindings from every branch that can reach the statement after a conditional.
///
/// A real PHP local absent from one reachable branch remains a possible runtime binding but has no
/// safe precise type, so it is widened to `Mixed`. Synthetic property-narrowing keys are different:
/// absence means the fact is not common to every path, so those keys are omitted. Bindings present
/// everywhere are widened to the normalized union of their exit types.
fn join_fallthrough_type_envs(checker: &Checker, branches: &[TypeEnv]) -> Option<TypeEnv> {
    branches.first()?;
    let mut names = branches
        .iter()
        .flat_map(|branch| branch.keys().cloned())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();

    let mut joined = TypeEnv::default();
    for name in names {
        let mut types = Vec::with_capacity(branches.len());
        for branch in branches {
            if let Some(ty) = branch.get(&name) {
                types.push(ty.clone());
            }
        }

        if types.len() != branches.len() {
            if !name.starts_with('\u{1}') {
                joined.insert(name, PhpType::Mixed);
            }
        } else {
            joined.insert(name, join_fallthrough_binding_types(checker, &types));
        }
    }
    Some(joined)
}

/// Joins one binding's types across reachable branch exits without losing container shape.
///
/// A compatible supertype wins when it accepts every observed value. Differing array element
/// types widen inside the same array container, because representing them as a union of whole
/// arrays would collapse to boxed `Mixed` storage in codegen and make subsequent element access
/// or reference binding lose the fact that the runtime value is definitely an array.
fn join_fallthrough_binding_types(checker: &Checker, types: &[PhpType]) -> PhpType {
    let mut joined = types.first().cloned().unwrap_or(PhpType::Never);
    for next in types.iter().skip(1) {
        joined = if checker.type_accepts(&joined, next) {
            joined
        } else if checker.type_accepts(next, &joined) {
            next.clone()
        } else {
            match (&joined, next) {
                (PhpType::Array(left), PhpType::Array(right)) => PhpType::Array(Box::new(
                    PhpType::widen_array_branch_element((**left).clone(), (**right).clone()),
                )),
                (
                    PhpType::AssocArray {
                        key: left_key,
                        value: left_value,
                    },
                    PhpType::AssocArray {
                        key: right_key,
                        value: right_value,
                    },
                ) => PhpType::AssocArray {
                    key: Box::new(PhpType::widen_array_branch_element(
                        (**left_key).clone(),
                        (**right_key).clone(),
                    )),
                    value: Box::new(PhpType::widen_array_branch_element(
                        (**left_value).clone(),
                        (**right_value).clone(),
                    )),
                },
                _ => checker.normalize_union_type(vec![joined, next.clone()]),
            }
        };
    }
    joined
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
                // Recorded before the iterable is inferred so an iterable that fails to type is
                // still treated as aliased.
                //
                // Both names are REF-BOUND rather than merely lent: `$v` is a cell
                // `typed_foreach` keeps alive across the whole loop (and past it, as PHP
                // leaves it bound to the last element), and the iterable holds the elements
                // those cells point into. Neither slot may be re-represented by a widening.
                if *value_by_ref {
                    self.record_ref_bound_alias_root(array);
                    self.ref_aliased_locals.insert(value_var.clone());
                    self.non_builtin_ref_aliased_locals
                        .insert(value_var.clone());
                    self.ref_bound_locals.insert(value_var.clone());
                    // Same rule as `=&`: a by-reference foreach binds the element cell,
                    // which is not the boxed capture cell the exemption is about.
                    self.by_ref_capture_boxed_locals.remove(value_var);
                }
                let value_was_bound = env.contains_key(value_var);
                let arr_ty = self.infer_type_with_assignment_effects(array, env)?;
                let warning_site = (self.current_loop_storage_scope.clone(), stmt.span);
                self.foreach_warnings.remove(&warning_site);
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
                    if let Some(k) = key_var {
                        env.insert(k.clone(), key_ty.clone());
                        self.clear_foreach_callable_metadata(k);
                    }
                    // A fresh by-reference binding is null when an empty source never enters the
                    // body, and later writes through the live reference may change its PHP type.
                    // Preserve an existing binding's stricter write-through contract, but give a
                    // fresh binding the Mixed shape used by the runtime reference slot.
                    let value_ty = if *value_by_ref && !value_was_bound {
                        PhpType::Mixed
                    } else {
                        *elem_ty.clone()
                    };
                    env.insert(value_var.clone(), value_ty.clone());
                    self.update_foreach_callable_metadata(value_var, array, &value_ty);
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
                    if let Some(k) = key_var {
                        env.insert(k.clone(), *key.clone());
                        self.clear_foreach_callable_metadata(k);
                    }
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
                    env.insert(value_var.clone(), value_ty.clone());
                    self.update_foreach_callable_metadata(value_var, array, &value_ty);
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
                    let (key_ty, value_ty) = if !is_iter
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
                    if let Some(k) = key_var {
                        env.insert(k.clone(), key_ty);
                        self.clear_foreach_callable_metadata(k);
                    }
                    env.insert(value_var.clone(), value_ty);
                    self.clear_foreach_callable_metadata(value_var);
                } else if matches!(
                    arr_ty,
                    PhpType::Iterable | PhpType::Mixed | PhpType::Union(_) | PhpType::Never
                ) {
                    // `Never` is a source that cannot produce a value (`foreach ((throw $e) as ...)`):
                    // the loop body is dead, but it is still checked like any other walk.
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Mixed);
                        self.clear_foreach_callable_metadata(k);
                    }
                    env.insert(value_var.clone(), PhpType::Mixed);
                    self.clear_foreach_callable_metadata(value_var);
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
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Mixed);
                        self.clear_foreach_callable_metadata(k);
                    }
                    env.insert(value_var.clone(), PhpType::Mixed);
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
                if *value_by_ref && matches!(arr_ty, PhpType::Object(_) | PhpType::Iterable) {
                    return Err(CompileError::new(
                        stmt.span,
                        "by-reference foreach over Iterator/IteratorAggregate objects or iterable-typed values is not supported; use an array source or remove &",
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
                stabilize_loop_storage(self, stmt.span, body, None, ref_value_binding, env, None);
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
                let errors = self.check_break_continue_target_body(body, env);
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
                for (values, _) in cases {
                    for v in values {
                        self.infer_type_with_assignment_effects(v, env)?;
                    }
                }
                let direct_entry_env = env.clone();
                let mut switch_exit_envs = Vec::new();
                let mut fallthrough_env = None;
                self.break_continue_depth += 1;
                self.loop_break_envs.push(Vec::new());
                for (_, body) in cases {
                    let invalidated = self
                        .conditional_boxed_ref_invalidations
                        .last()
                        .cloned()
                        .unwrap_or_default();
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs
                        .iter()
                        .filter(|name| !invalidated.contains(*name))
                        .cloned()
                        .collect();
                    let mut entry_envs = vec![direct_entry_env.clone()];
                    if let Some(previous) = fallthrough_env.take() {
                        entry_envs.push(previous);
                    }
                    let mut branch_env = join_fallthrough_type_envs(self, &entry_envs)
                        .unwrap_or_else(|| direct_entry_env.clone());
                    errors.extend(self.check_body(body, &mut branch_env));
                    match block_terminal_effect_with_divergence(body, &|expr| {
                        self.expr_is_declared_never_call(expr)
                    }) {
                        TerminalEffect::FallsThrough => fallthrough_env = Some(branch_env),
                        TerminalEffect::Breaks | TerminalEffect::TerminatesMixed => {
                            switch_exit_envs.push(branch_env);
                        }
                        TerminalEffect::ExitsCurrentBlock => {}
                    }
                }
                if let Some(body) = default {
                    let invalidated = self
                        .conditional_boxed_ref_invalidations
                        .last()
                        .cloned()
                        .unwrap_or_default();
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs
                        .into_iter()
                        .filter(|name| !invalidated.contains(name))
                        .collect();
                    let mut entry_envs = vec![direct_entry_env.clone()];
                    if let Some(previous) = fallthrough_env.take() {
                        entry_envs.push(previous);
                    }
                    let mut branch_env = join_fallthrough_type_envs(self, &entry_envs)
                        .unwrap_or_else(|| direct_entry_env.clone());
                    errors.extend(self.check_body(body, &mut branch_env));
                    match block_terminal_effect_with_divergence(body, &|expr| {
                        self.expr_is_declared_never_call(expr)
                    }) {
                        TerminalEffect::FallsThrough => switch_exit_envs.push(branch_env),
                        TerminalEffect::Breaks | TerminalEffect::TerminatesMixed => {
                            switch_exit_envs.push(branch_env);
                        }
                        TerminalEffect::ExitsCurrentBlock => {}
                    }
                } else {
                    switch_exit_envs.push(direct_entry_env);
                    if let Some(previous) = fallthrough_env.take() {
                        switch_exit_envs.push(previous);
                    }
                }
                // A case whose LAST statement breaks is already an exit above; a `break` nested
                // deeper in a case (inside an `if`) is only seen here.
                switch_exit_envs.extend(self.loop_break_envs.pop().unwrap_or_default());
                self.break_continue_depth -= 1;
                self.callable_array_targets.clear();
                self.callable_array_target_versions.clear();
                if let Some(joined) = join_fallthrough_type_envs(self, &switch_exit_envs) {
                    *env = joined;
                }
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
                let mut errors = Vec::new();

                // Each clause body receives its own environment. The shared `env` carries only the
                // accumulated false-condition path into the next elseif/else; mutating it while
                // checking a truthy sibling would leak assignments between mutually exclusive
                // branches. Reachable exits are joined after the whole chain.
                let mut clauses: Vec<(&Expr, &Vec<Stmt>)> = vec![(condition, then_body)];
                clauses.extend(elseif_clauses.iter().map(|(c, b)| (c, b)));
                let mut fallthrough_clause_envs = Vec::new();
                // Pre-guard types held for `merge_local_assignment_type` so a store inside a guarded
                // region is measured against the BINDING rather than against the guard's view of
                // the value (issue #509). Closed once for the whole construct: the complement a
                // guarded clause publishes narrows the later clauses and the `else` body too.
                let mut saved_origins: Vec<SavedNarrowingOrigin> = Vec::new();
                let mut callable_target_exits = Vec::new();
                let mut callable_target_version_exits = Vec::new();

                for (cond, body) in &clauses {
                    self.infer_type_with_assignment_effects(cond, env)?;
                    let branch_entry_targets = self.callable_array_targets.clone();
                    let branch_entry_target_versions =
                        self.callable_array_target_versions.clone();
                    let branch_entry_boxed_refs = self.boxed_ref_aliased_locals.clone();
                    let condition_env = env.clone();
                    let mut branch_env = condition_env.clone();
                    self.install_truthy_short_circuit_effects(cond, &mut branch_env)?;

                    let mut published_else: Option<(String, PhpType)> = None;
                    // `isset($never)` binds `$never` for the true branch only. Narrowing it would
                    // publish the false edge's `null` as a binding after the `if`, and PHP's
                    // `isset()` creates nothing.
                    let isset_roots = Self::truthy_isset_fact_roots(cond, &condition_env);
                    let guard = self
                        .guard_narrowing(cond, &branch_env)?
                        .filter(|guard| !isset_roots.contains(&guard.var));
                    if let Some(guard) = guard {
                        self.box_iterable_local_narrowed_to_array(&guard, &condition_env);
                        saved_origins.push(self.enter_flow_narrowing(
                            &guard.var,
                            condition_env.get(&guard.var),
                            &guard.then_ty,
                        ));
                        branch_env.insert(guard.var.clone(), guard.then_ty);
                        *env = condition_env;
                        env.insert(guard.var.clone(), guard.else_ty.clone());
                        published_else = Some((guard.var, guard.else_ty));
                    } else {
                        *env = condition_env;
                    }

                    for s in *body {
                        if let Err(error) = self.check_stmt(s, &mut branch_env) {
                            errors.extend(error.flatten());
                        }
                    }
                    // A probe root the condition proved set is a fact of this branch only.
                    Self::drop_truthy_isset_root_facts(cond, env, &mut branch_env);
                    let falls_through = !self.body_cannot_fall_through(body);
                    if falls_through {
                        callable_target_exits.push(self.callable_array_targets.clone());
                        callable_target_version_exits
                            .push(self.callable_array_target_versions.clone());
                    }
                    self.callable_array_targets = branch_entry_targets;
                    self.callable_array_target_versions = branch_entry_target_versions;
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs;
                    if let Some((var, else_ty)) = &published_else {
                        self.republish_flow_narrowing(var, else_ty);
                    }
                    if falls_through {
                        fallthrough_clause_envs.push(branch_env);
                    }
                    self.install_falsy_short_circuit_effects(cond, env)?;
                }

                // The final else receives the accumulated complements without mutating the
                // already-recorded truthy exits. With no explicit else, that false path itself is
                // a reachable exit.
                let mut else_falls_through = else_body.is_none();
                if let Some(body) = else_body {
                    let mut branch_env = env.clone();
                    for s in body {
                        if let Err(error) = self.check_stmt(s, &mut branch_env) {
                            errors.extend(error.flatten());
                        }
                    }
                    else_falls_through = !self.body_cannot_fall_through(body);
                    if else_falls_through {
                        fallthrough_clause_envs.push(branch_env);
                    }
                } else {
                    fallthrough_clause_envs.push(env.clone());
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
                if let Some(joined) = join_fallthrough_type_envs(self, &fallthrough_clause_envs) {
                    *env = joined;
                }
                // Closed for the construct even when the complement is KEPT for the code after
                // it. A kept complement is a sound fact about every path that gets there, so the
                // statements after the `if` are ordinary straight-line code: a store that does
                // not fit takes the depth-0 re-bind path, which abandons the slot properly,
                // rather than being waved through against a binding the `if` may have replaced.
                for saved_origin in saved_origins.into_iter().rev() {
                    self.exit_flow_narrowing(saved_origin);
                }

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::DoWhile { body, condition } => {
                self.boxed_ref_aliased_locals.clear();
                stabilize_loop_storage(self, stmt.span, body, None, None, env, None);
                let errors = self.check_break_continue_target_body(body, env);
                self.infer_type_with_assignment_effects(condition, env)?;
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
                stabilize_loop_storage(self, stmt.span, body, None, None, env, None);
                self.infer_type_with_assignment_effects(condition, env)?;
                let truthy_short_circuit_bindings =
                    self.install_truthy_short_circuit_effects(condition, env)?;
                // The condition is re-evaluated before every iteration, so a guard on it
                // holds on entry to each one: `while (($row = fgetcsv($h)) !== false)`
                // leaves `$row` an array inside the body. The narrowing is dropped again
                // afterwards, because the loop exits precisely when the guard is false.
                // Installed INSIDE the short-circuit effects and undone before them, so the
                // two restores nest instead of overwriting one another.
                let guard = self.guard_narrowing(condition, env)?;
                let saved = guard
                    .as_ref()
                    .map(|g| (g.var.clone(), env.get(&g.var).cloned()));
                let saved_origin = guard
                    .as_ref()
                    .map(|g| self.enter_flow_narrowing(&g.var, env.get(&g.var), &g.then_ty));
                if let Some(g) = &guard {
                    env.insert(g.var.clone(), g.then_ty.clone());
                }
                let errors = self.check_break_continue_target_body(body, env);
                if let Some(saved_origin) = saved_origin {
                    self.exit_flow_narrowing(saved_origin);
                }
                if let Some((var, previous)) = saved {
                    match previous {
                        Some(ty) => {
                            env.insert(var, ty);
                        }
                        None => {
                            env.remove(&var);
                        }
                    }
                }
                for (name, previous) in &truthy_short_circuit_bindings {
                    restore_narrowed_var(env, name, previous);
                }
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
                    None,
                    env,
                    packed_counter_name.as_deref(),
                );
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
                match thrown_ty {
                    PhpType::Object(type_name)
                        if self.object_type_implements_throwable(&type_name)
                            || self.unresolved_new_object_defers_to_runtime(expr, &type_name)
                            || self.absent_class_defers_to_runtime(&type_name)
                            || self.object_type_may_be_eval_declared(&type_name) =>
                    {
                        Ok(())
                    }
                    PhpType::Mixed if self.eval_barrier_active => Ok(()),
                    PhpType::Object(_) => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    // `Never` reaches here only from a guard the checker has just proven
                    // cannot be taken, so the statement is unreachable rather than wrong.
                    PhpType::Never => Ok(()),
                    ref ty
                        if crate::types::checker::type_compat::type_is_gradual_object_family(ty) =>
                    {
                        Ok(())
                    }
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let mut errors = Vec::new();
                // A `catch` runs because some statement in the try RAISED, so it must see what
                // the variables held at a point where that could happen: the environment BEFORE
                // each statement. Checking the catches against the environment the try body LEFT
                // says every assignment completed — the one thing that cannot be true of the
                // statement that threw.
                //
                // Symfony's `ClassStub::__construct` is the shape that exposed it:
                //
                //     if (\is_array($r)) {
                //         try { $r = new \ReflectionMethod($r[0], $r[1]); }
                //         catch (\ReflectionException) { $r = new \ReflectionClass($r[0]); }
                //     }
                //
                // The catch indexes `$r`, which is still the array — the store never happened —
                // but the checker had already retyped it to `ReflectionMethod` and refused with
                // "Cannot index non-array".
                //
                // The env AFTER the last statement is deliberately NOT joined in: if every
                // statement completed, nothing threw and no catch runs.
                //
                // Residual imprecision, in exchange: a statement that completes a nested
                // assignment and only then raises (`foo($r = 1, throws())`) reaches its catch
                // with the pre-statement type. The previous behaviour had the mirror-image
                // problem for every ordinary assignment, which is the common shape.
                let mut throw_point_envs = Vec::with_capacity(try_body.len().max(1));
                let branch_entry_boxed_refs = self.boxed_ref_aliased_locals.clone();
                for s in try_body {
                    throw_point_envs.push(env.clone());
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                let catch_env = join_fallthrough_type_envs(self, &throw_point_envs).map(
                    |mut joined| {
                        // A name the TRY INTRODUCED is not in any throw-point environment, but it
                        // can still be live in the catch: PHP binds a BY-REFERENCE argument at the
                        // call, before the callee body can raise. `HttpKernel::handle` does
                        // exactly that with `$controllerMetadata`. Such a name can only have come
                        // from the try, so it keeps the type the try left.
                        for (name, ty) in env.iter() {
                            if !joined.contains_key(name) {
                                joined.insert(name.clone(), ty.clone());
                            }
                        }
                        joined
                    },
                );
                // Every way the statement can finish: the try body completing, or one of the
                // catches completing. They are joined on the way out so a catch's bindings —
                // including its own exception variable, which PHP leaves defined afterwards —
                // survive the statement.
                let mut exit_envs = vec![env.clone()];
                for catch_clause in catches {
                    let mut catch_scope = match &catch_env {
                        Some(joined) => joined.clone(),
                        None => env.clone(),
                    };
                    let env = &mut catch_scope;
                    self.boxed_ref_aliased_locals = branch_entry_boxed_refs.clone();
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
                    if let Some(variable) = &catch_clause.variable {
                        env.insert(
                            variable.clone(),
                            PhpType::Object(self.common_catch_type_name(&resolved_types)),
                        );
                    }
                    for s in &catch_clause.body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                    if !self.body_cannot_fall_through(&catch_clause.body) {
                        exit_envs.push(catch_scope);
                    }
                }
                if let Some(joined) = join_fallthrough_type_envs(self, &exit_envs) {
                    *env = joined;
                }
                if let Some(body) = finally_body {
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

    /// Checks a loop body with `break`/`continue` target tracking.
    ///
    /// Increments `break_continue_depth` before checking the body and decrements it after,
    /// so that `break`/`continue` validation knows the correct nesting level. Returns all
    /// errors accumulated while checking the body; the caller decides whether to propagate them.
    /// Checks a `for` loop's condition, update, and body re-walk, after its storage has settled.
    fn check_for_tail(
        &mut self,
        condition: Option<&Expr>,
        update: Option<&Stmt>,
        body: &[Stmt],
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        if let Some(c) = condition {
            self.infer_type_with_assignment_effects(c, env)?;
        }
        if let Some(s) = update {
            self.check_stmt(s, env)?;
        }
        let errors = self.check_break_continue_target_body(body, env);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(CompileError::from_many(errors))
        }
    }

    /// Checks a loop body with its control-flow depth and conservative property narrowings.
    fn check_break_continue_target_body(
        &mut self,
        body: &[Stmt],
        env: &mut TypeEnv,
    ) -> Vec<CompileError> {
        // A loop body may observe a property write made by an earlier iteration (the write site
        // is *after* the read site in source order), so property narrowings from outside the
        // loop cannot be trusted inside it.
        Self::purge_property_narrowings(env);
        self.break_continue_depth += 1;
        self.loop_break_envs.push(Vec::new());
        let errors = self.check_body(body, env);
        let breaks = self.loop_break_envs.pop().unwrap_or_default();
        self.break_continue_depth -= 1;
        // The loop is left by its own condition (the environment the body ends with, which is
        // also what the next condition check sees) and by every `break` that targets it.
        if !breaks.is_empty() {
            let mut exits = Vec::with_capacity(breaks.len() + 1);
            exits.push(env.clone());
            exits.extend(breaks);
            if let Some(joined) = join_fallthrough_type_envs(self, &exits) {
                *env = joined;
            }
        }
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
        let mut errors = Vec::new();
        for stmt in body {
            if let Err(error) = self.check_stmt(stmt, env) {
                errors.extend(error.flatten());
            }
        }
        errors
    }
}
