//! Purpose:
//! Type-checks assignment properties forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{
    is_compound_property_assignment_self_read, CastType, Expr, ExprKind,
};
use crate::span::Span;
use crate::types::param_binding::param_accepts_weak_string_coercion;
use crate::types::{
    merge_array_key_types, normalized_array_key_type, static_array_key_forces_hash_storage,
    PhpType, TypeEnv,
};

use super::super::super::scope_dynamic_storage;
use super::super::super::Checker;
use super::properties_null_coalesce::null_coalesce_property_targets_same_slot;

/// Type-checks a direct property assignment (`$obj->prop = value`).
///
/// Infers the object and value types, then validates write access for `Object` and `Pointer` types.
/// For objects, also refines the property's inferred type based on the assigned value.
pub(super) fn check_property_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
    compound_property_update: bool,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    // Capture the checked receiver view before RHS inference can invalidate property guards.
    checker.record_member_receiver_type(span, &obj_ty);
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let storage_ty = property_assignment_storage_type(value, &val_ty);
    if let PhpType::Object(class_name) = &obj_ty {
        check_object_property_write(checker, object, class_name, property, value, &val_ty, span, compound_property_update)?;
        refine_object_property_type(checker, class_name, property, &storage_ty);
    } else if let Some(class_name) = checker.union_single_object_class(&obj_ty) {
        // A factory-style `Object|false` receiver still targets the one object
        // class when the runtime value is an object. Validate writes against that
        // class so readonly/visibility/type rules are not silently bypassed merely
        // because the success value has not yet been narrowed with instanceof.
        check_object_property_write(checker, object, &class_name, property, value, &val_ty, span, compound_property_update)?;
        refine_object_property_type(checker, &class_name, property, &storage_ty);
    }
    // A boxed `Mixed` receiver names no class, so the reservation is recorded for every class
    // whose layout carries THIS name as a strict ancestor's private slot. Without it the Mixed
    // write ladder had no arm for such a class and the store fell through to the stdClass-only
    // miss helper, which dropped it with no diagnostic anywhere.
    if matches!(obj_ty.codegen_repr(), PhpType::Mixed) {
        scope_dynamic_storage::record_scope_dynamic_mixed_receiver_mutation(checker, property);
    }
    // A runtime-shaped value written through a receiver whose class the checker cannot name may
    // land in ANY class's untyped slot of this name; a refined raw slot cannot take the box (the
    // backend refuses it at run time), so those slots widen exactly as for an `unset()`.
    if crate::types::checker::single_object_class_name(&obj_ty).is_none()
        && matches!(obj_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_))
        && matches!(storage_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        crate::types::checker::clone_override_storage::record_property_unset_destination(
            checker,
            &obj_ty,
            Some(property),
        );
    }
    if let PhpType::Pointer(Some(class_name)) | PhpType::Packed(class_name) = &obj_ty {
        check_pointer_property_write(checker, class_name, property, &val_ty, span)?;
    }
    Ok(())
}

/// Type-checks a declared-property reference bind and promotes both property slots to cells.
pub(super) fn check_property_ref_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    source: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(source, env)?;
    let target_class = if let PhpType::Object(class_name) = &obj_ty {
        check_object_property_write(checker, object, class_name, property, source, &val_ty, span, false)?;
        refine_object_property_type(checker, class_name, property, &val_ty);
        Some(class_name.clone())
    } else if let Some(class_name) = checker.union_single_object_class(&obj_ty) {
        check_object_property_write(checker, object, &class_name, property, source, &val_ty, span, false)?;
        refine_object_property_type(checker, &class_name, property, &val_ty);
        Some(class_name)
    } else {
        None
    };
    let target_class_for_binding = target_class.clone();
    if let Some(class_name) = target_class {
        checker
            .reference_property_promotions
            .insert((class_name, property.to_string()));
    }
    // A static source binds the property to the static's own storage (see the parser's
    // `parse_complex_ref_assignment`); the property is promoted above, the static needs nothing.
    if matches!(source.kind, ExprKind::StaticPropertyAccess { .. }) {
        return Ok(());
    }
    let ExprKind::PropertyAccess {
        object: source_object,
        property: source_property,
    } = &source.kind
    else {
        return Err(CompileError::new(
            source.span,
            "Property reference binding currently requires a declared-property source",
        ));
    };
    let source_object_ty = checker.infer_type(source_object, env)?;
    if let Some(class_name) = crate::types::checker::single_object_class_name(&source_object_ty) {
        checker
            .reference_property_promotions
            .insert((class_name.clone(), source_property.clone()));
        if let Some(target_class) = target_class_for_binding {
            checker.reference_property_bindings.push((
                target_class,
                property.to_string(),
                class_name,
                source_property.clone(),
                span,
            ));
        }
    }
    Ok(())
}

/// Returns the backend storage contract of a whole-value property assignment.
///
/// Gradual `array_merge` and `(array)` expressions can produce indexed or associative storage
/// with heterogeneous values. Their EIR result is therefore `array<mixed>` even when closed-world
/// inference can describe the current inputs more narrowly; an untyped property must follow the
/// materialized representation so later loads and cleanup use the correct slot layout.
fn property_assignment_storage_type(value: &Expr, inferred: &PhpType) -> PhpType {
    let uses_gradual_array_storage = match &value.kind {
        ExprKind::Cast {
            target: CastType::Array,
            ..
        } => true,
        ExprKind::FunctionCall { name, .. } => php_symbol_key(name.as_str()) == "array_merge",
        _ => false,
    };
    if uses_gradual_array_storage {
        PhpType::Array(Box::new(PhpType::Mixed))
    } else {
        inferred.clone()
    }
}

/// Type-checks an array-push property operation (`$obj->prop[] = value`).
///
/// Infers object and value types, then validates that the property is an array (not a buffer).
/// For `PhpType::Object`, resolves the property type and computes the updated array element type,
/// merging element types if the property has no declared type.
pub(super) fn check_property_array_push(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    // A gradual receiver (`$batches[$i]->entities[] = $e`, the element of an untyped array) is
    // resolved at run time, exactly as the keyed write `$x->prop[$k] = $v` already is.
    if matches!(obj_ty, PhpType::Mixed) {
        return Ok(());
    }
    let obj_ty = checker.union_single_object_class(&obj_ty)
        .filter(|_| matches!(&obj_ty, PhpType::Union(members)
            if members.iter().all(|ty| matches!(ty, PhpType::Object(_) | PhpType::Void))))
        .map(PhpType::Object).unwrap_or(obj_ty);
    // So is a union of object types, which a property chain keeps after an `instanceof` test
    // the checker does not narrow through (`$stmt->whereClause->conditionalExpression
    // ->conditionalFactors[] = $primary` in Doctrine's pagination walkers).
    if let PhpType::Union(members) = &obj_ty {
        if members.iter().any(|member| matches!(member, PhpType::Object(_)))
            && members
                .iter()
                .all(|member| matches!(member, PhpType::Object(_) | PhpType::Void))
        {
            return Ok(());
        }
    }
    match &obj_ty {
        PhpType::Object(class_name) => {
            if class_name.trim_start_matches('\\').is_empty()
                || receiver_is_interface_only(checker, class_name)
            {
                check_generic_object_property_array_push(
                    checker, property, &val_ty, span,
                )?;
                return Ok(());
            }
            if object_property_write_is_dynamic(checker, class_name, property) {
                return Ok(());
            }
            refuse_scope_dynamic_element_write(checker, class_name, property, span)?;
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            // `$this->lines[] = $line` on an `ArrayAccess` property is `offsetSet(null, $line)`
            // (league/commonmark's block parsers hold an `ArrayCollection` there), as the keyed
            // form already accepts.
            if matches!(&prop_ty, PhpType::Object(name) if name.eq_ignore_ascii_case("WeakMap"))
                || type_satisfies_array_access(checker, &prop_ty)
            {
                return Ok(());
            }
            let updated_prop_ty = updated_array_property_push_type(
                checker,
                &prop_ty,
                property_has_declared_type,
                class_name,
                property,
                &val_ty,
                span,
            )?;
            update_object_property_type(
                checker,
                class_name,
                property,
                property_has_declared_type,
                updated_prop_ty,
            );
            Ok(())
        }
        PhpType::Pointer(Some(class_name)) => {
            let field_ty = resolve_pointer_field_type(checker, class_name, property, span, "Array push")?;
            match field_ty {
                PhpType::Array(_) => Ok(()),
                PhpType::Buffer(_) => Err(CompileError::new(
                    span,
                    "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                )),
                other => Err(CompileError::new(
                    span,
                    &format!("Array push requires an array property, got {}", other),
                )),
            }
        }
        _ => Err(CompileError::new(
            span,
            "Array push requires an object or typed pointer",
        )),
    }
}

/// Returns whether the class, or an ancestor, declares one magic accessor.
///
/// `ClassInfo::methods` is already flattened over the ancestry, so an inherited accessor counts,
/// which is the same reading the rest of this check and `magic_set_receiver_has_method` use.
fn class_declares_method(checker: &Checker, class_name: &str, method: &str) -> bool {
    checker
        .classes
        .get(class_name.trim_start_matches('\\'))
        .is_some_and(|class_info| class_info.methods.contains_key(method))
}

/// Refuses creating a dynamic property on a `readonly` class, which php forbids outright.
///
/// A `readonly` class carries php's no-dynamic-properties flag, so a name php resolves to a
/// dynamic property there is not stored at all: php raises `Cannot create dynamic property
/// C::$p`. The scope-dynamic arm above answers before the whole visibility and readonly ladder
/// runs, so without this the arm would have created on a class that may not hold anything, and
/// `scope_dynamic_storage` would have reserved a hash no legal write can ever fill.
fn refuse_dynamic_property_on_readonly_class(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
) -> Result<(), CompileError> {
    let readonly = checker
        .classes
        .get(class_name.trim_start_matches('\\'))
        .is_some_and(|class_info| class_info.is_readonly_class);
    if !readonly {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!(
            "Cannot create dynamic property {}::${}",
            class_name.trim_start_matches('\\'),
            property
        ),
    ))
}

/// Refuses an element write through a name php does NOT resolve to a declared slot here.
///
/// `$obj->p[] = v` and `$obj->p[$k] = v` read the property's container, mutate it and publish it
/// back, which needs storage the element write can address for the whole operation. For a name php
/// resolves to a dynamic property, or one it refuses outright, the container is not a slot at all,
/// and resolving one anyway would select a strict ancestor's private storage and then refine its
/// inferred type from a write that never reaches it. Refusing keeps both from happening.
fn refuse_scope_dynamic_element_write(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
) -> Result<(), CompileError> {
    if !scope_dynamic_storage::mutation_targets_scope_dynamic_name(checker, class_name, property) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!(
            "Cannot write an array element through {}::{} from this scope: PHP resolves that \
             name to a dynamic property here",
            class_name, property
        ),
    ))
}

/// Type-checks an indexed property array assignment (`$obj->prop[$index] = value`).
///
/// Infers object, index, and value types. Validates array key types and property mutability.
/// For `PhpType::Object`, handles array storage and buffer element validation, merging array
/// element types when the property lacks a declared type. For `PhpType::Pointer`, requires
/// integer indexing and that the field is an array type.
pub(super) fn check_property_array_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let obj_ty = checker.union_single_object_class(&obj_ty)
        .filter(|_| matches!(&obj_ty, PhpType::Union(members)
            if members.iter().all(|ty| matches!(ty, PhpType::Object(_) | PhpType::Void))))
        .map(PhpType::Object).unwrap_or(obj_ty);
    if matches!(obj_ty, PhpType::Mixed) {
        if !is_php_array_key_type(&normalized_idx_ty) {
            return Err(CompileError::new(span, "Array index must be integer"));
        }
        return Ok(());
    }
    // A nullable object receiver (`protected ResultSetMapping|null $rsm`) writes through its
    // object member; a null one is php's run-time "Attempt to assign property on null".
    // doctrine/orm's `AbstractHydrator::gatherRowData()` writes `$this->rsm->newObject[$i]`, and
    // the refusal interpreted AbstractHydrator and the six hydrators extending it.
    let obj_ty = match &obj_ty {
        PhpType::Union(members) => {
            let objects: Vec<&PhpType> = members
                .iter()
                .filter(|member| !matches!(member, PhpType::Void))
                .collect();
            match objects.as_slice() {
                [only @ PhpType::Object(_)] if objects.len() + 1 == members.len() => (*only).clone(),
                _ => obj_ty.clone(),
            }
        }
        _ => obj_ty.clone(),
    };
    match &obj_ty {
        PhpType::Object(class_name) => {
            // An interface declares no property: the object behind it does, which php finds when
            // the write runs (`$metadata->table['indexes'] = []` through Doctrine's
            // `Persistence\Mapping\ClassMetadata` in the ORM's `XmlDriver`). It was "Undefined
            // class", and the driver went to the interpreter.
            if class_name.trim_start_matches('\\').is_empty()
                || receiver_is_interface_only(checker, class_name)
            {
                if !is_php_array_key_type(&normalized_idx_ty) {
                    return Err(CompileError::new(span, "Array index must be integer"));
                }
                check_generic_object_property_array_assign(
                    checker,
                    property,
                    index,
                    &normalized_idx_ty,
                    &val_ty,
                    span,
                )?;
                return Ok(());
            }
            if object_property_write_is_dynamic(checker, class_name, property) {
                if !is_php_array_key_type(&normalized_idx_ty) {
                    return Err(CompileError::new(span, "Array index must be integer"));
                }
                return Ok(());
            }
            refuse_scope_dynamic_element_write(checker, class_name, property, span)?;
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            if prop_ty == PhpType::Str {
                return Err(CompileError::new(
                    span,
                    super::arrays::STRING_OFFSET_ON_PROPERTY_UNSUPPORTED,
                ));
            }
            if type_satisfies_array_access(checker, &prop_ty) {
                return Ok(());
            }
            // An untyped property holds `mixed`, and a `mixed` container may be an `ArrayAccess`
            // object whose offsets are objects: Laravel's exception handler declares
            // `protected $reportedExceptionMap;`, fills it with `new WeakMap` in the constructor,
            // and writes `$this->reportedExceptionMap[$e] = true;`. Only an object index is
            // admitted here; lowering dispatches an `ArrayAccess` receiver to `offsetSet()` and
            // raises php's TypeError for anything else.
            // A closure is an object key too (`$this->scope[$closure]`, typed `callable`).
            let object_index = matches!(normalized_idx_ty, PhpType::Object(_) | PhpType::Callable);
            if matches!(prop_ty, PhpType::Mixed) && object_index {
                return Ok(());
            }
            // The same for an untyped property its writes refined to an `ArrayAccess` object or
            // null: laravel/serializable-closure's `Native` assigns `$this->scope = new
            // ClosureScope()` (an `SplObjectStorage`) or null, and writes
            // `$this->scope[$closure] = $instance`.
            if !property_has_declared_type
                && object_index
                && matches!(&prop_ty, PhpType::Union(members) if members.iter().any(|member| {
                    !matches!(member, PhpType::Void) && type_satisfies_array_access(checker, member)
                }))
            {
                return Ok(());
            }
            if let PhpType::Buffer(elem_ty) = &prop_ty {
                super::arrays::validate_buffer_element_write(elem_ty, &idx_ty, &val_ty, span)?;
                return Ok(());
            }
            if !is_php_array_key_type(&normalized_idx_ty) {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            let updated_prop_ty = updated_array_property_assign_type(
                checker,
                &prop_ty,
                property_has_declared_type,
                class_name,
                property,
                index,
                &normalized_idx_ty,
                &val_ty,
                span,
            )?;
            update_object_property_type(
                checker,
                class_name,
                property,
                property_has_declared_type,
                updated_prop_ty,
            );
            Ok(())
        }
        PhpType::Pointer(Some(class_name)) => {
            let field_ty = resolve_pointer_field_type(
                checker,
                class_name,
                property,
                span,
                "Array index assignment",
            )?;

            if !matches!(normalized_idx_ty, PhpType::Int) {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            match field_ty {
                PhpType::Array(_) => Ok(()),
                other => Err(CompileError::new(
                    span,
                    &format!(
                        "Array index assignment requires an array property, got {}",
                        other
                    ),
                )),
            }
        }
        _ => Err(CompileError::new(
            span,
            "Array index assignment requires an object or typed pointer",
        )),
    }
}

/// Validates an append through PHP's bare `object` type against compatible AOT property owners.
fn check_generic_object_property_array_push(
    checker: &mut Checker,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    let candidates = generic_object_array_property_candidates(checker, property);
    for (class_name, prop_ty, property_has_declared_type) in candidates {
        if matches!(prop_ty.codegen_repr(), PhpType::Object(_) | PhpType::Mixed) {
            continue;
        }
        let updated = updated_array_property_push_type(
            checker,
            &prop_ty,
            property_has_declared_type,
            &class_name,
            property,
            val_ty,
            span,
        )?;
        update_object_property_type(
            checker,
            &class_name,
            property,
            property_has_declared_type,
            updated,
        );
    }
    Ok(())
}

/// Validates a keyed write through PHP's bare `object` type against compatible AOT owners.
fn check_generic_object_property_array_assign(
    checker: &mut Checker,
    property: &str,
    index: &Expr,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    let candidates = generic_object_array_property_candidates(checker, property);
    for (class_name, prop_ty, property_has_declared_type) in candidates {
        if matches!(prop_ty.codegen_repr(), PhpType::Object(_) | PhpType::Mixed) {
            continue;
        }
        let updated = updated_array_property_assign_type(
            checker,
            &prop_ty,
            property_has_declared_type,
            &class_name,
            property,
            index,
            normalized_idx_ty,
            val_ty,
            span,
        )?;
        update_object_property_type(
            checker,
            &class_name,
            property,
            property_has_declared_type,
            updated,
        );
    }
    Ok(())
}

/// Collects array-like property owners that may receive a write through bare `object`.
fn generic_object_array_property_candidates(
    checker: &Checker,
    property: &str,
) -> Vec<(String, PhpType, bool)> {
    checker
        .classes
        .iter()
        .filter_map(|(class_name, class_info)| {
            let (_, (_, prop_ty)) = class_info.visible_property(property)?;
            let compatible = matches!(
                prop_ty.codegen_repr(),
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
            ) || matches!(
                prop_ty.codegen_repr(),
                PhpType::Object(class_name) if class_name.trim_start_matches('\\').is_empty()
            ) || type_satisfies_array_access(checker, prop_ty);
            compatible.then(|| {
                (
                    class_name.clone(),
                    prop_ty.clone(),
                    class_info.visible_property_is_declared(property),
                )
            })
        })
        .collect()
}

/// Validates a write to a named property of a class instance.
///
/// Checks: property existence, `__set` magic method fallback, dynamic properties (`#[\AllowDynamicProperties]`),
/// readonly initialization and overwrite rules,
/// visibility via `can_access_member`, and declared-type compatibility via `require_compatible_arg_type`.
/// StdClass properties are allowed unconditionally.
fn check_object_property_write(
    checker: &mut Checker,
    object: &Expr,
    class_name: &str,
    property: &str,
    value: &Expr,
    val_ty: &PhpType,
    span: Span,
    compound_property_update: bool,
) -> Result<(), CompileError> {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return Ok(());
    }
    // php 7.4 removed shadow properties. A strict ancestor's `private $p` lives under a mangled
    // key and the child's by-name table does not contain it at all, so a write outside the
    // declaring class CREATES a distinct dynamic property rather than being an access error.
    // `visible_property` below still finds that slot under its plain name, which is why this
    // arm has to answer first: without it the write was refused where php stores a value, and
    // relaxing the refusal alone would have let the write land in the ANCESTOR'S slot.
    if scope_dynamic_storage::mutation_targets_scope_dynamic_name(checker, class_name, property) {
        // php consults `__set` BEFORE it decides anything else about a name it does not resolve
        // to a visible slot, a `readonly` class included: the accessor answers and no dynamic
        // property is created, so there is nothing for the no-dynamic-properties rule to refuse
        // and nothing for the reservation to store. `record_scope_dynamic_mutation` skips such a
        // class too, so the two agree, but the refusal has to be skipped here explicitly or a
        // readonly class with `__set` would be rejected for a creation php never performs.
        let same_pair_reentry = scope_dynamic_storage::record_magic_set_same_pair_reentry(
            checker, object, class_name, property,
        );
        if same_pair_reentry || !class_declares_method(checker, class_name, "__set") {
            refuse_dynamic_property_on_readonly_class(checker, class_name, property, span)?;
        }
        if !same_pair_reentry {
            scope_dynamic_storage::record_scope_dynamic_mutation(
                checker, class_name, property, "__set",
            );
        }
        return Ok(());
    }
    if let Some(class_info) = checker.classes.get(class_name) {
        if class_info.visible_property(property).is_none() {
            if class_info.methods.contains_key("__set") {
                let same_pair_reentry =
                    scope_dynamic_storage::record_magic_set_same_pair_reentry(
                    checker, object, class_name, property,
                );
                if same_pair_reentry {
                    refuse_dynamic_property_on_readonly_class(
                        checker, class_name, property, span,
                    )?;
                }
                return Ok(());
            }
            if class_info.allow_dynamic_properties {
                // PHP 8.2 #[\AllowDynamicProperties]: writes to undeclared
                // properties are routed at codegen time to a per-object
                // hashtable side-table. The value is stored as `Mixed`.
                return Ok(());
            }
            // The write twin of the closure read rule in `infer_property_access`: a closure's
            // `$this` can be rebound to an unrelated class, and Laravel's `Signals::setHandlers()`
            // does `(fn () => $this->signalHandlers = $handlers)->call($this->registry)`. The
            // write is the bound receiver's to resolve at run time.
            if checker.closure_depth > 0 && matches!(object.kind, ExprKind::This) {
                return Ok(());
            }
            // The write twin of the read rule for a base whose DESCENDANT declares the property: the
            // receiver holds that descendant at run time. psysh's `IteratorRecursiveDirectory`
            // writes `$out->_relativePath` on the `RecursiveDirectoryIterator` its
            // `parent::getChildren()` is declared to return, which is a `new static`.
            if !checker.descendant_declared_property_types(class_name, property).is_empty() {
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!("Undefined property: {}::{}", class_name, property),
            ));
        }
        let expected_ty = class_info
            .visible_property(property)
            .map(|(_, (_, ty))| ty.clone())
            .unwrap_or(PhpType::Int);
        let readonly_null_coalesce = class_info.readonly_properties.contains(property)
            && null_coalesce_property_targets_same_slot(object, property, value);
        let internal_pdo_statement_initializer = checker
            .current_method
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("__elephcInitialize"))
            && class_info
                .property_declaring_classes
                .get(property)
                .is_some_and(|owner| owner.trim_start_matches('\\').eq_ignore_ascii_case("PDOStatement"));
        let declaring_class = class_info.property_declaring_classes.get(property)
            .map(String::as_str).unwrap_or(class_name).to_string();
        // Untyped readonly declarations are a legacy extension with implicitly initialized
        // null slots. Preserve their constructor exemption, not typed one-shot write semantics.
        let legacy_untyped_slot = class_info.visible_property(property).is_some_and(|(index, _)| {
            !class_info.property_slot_is_declared(index, property)
        });
        let readonly_write_guard = class_info.readonly_properties.contains(property)
            && !(checker.current_class.as_deref()
                == Some(declaring_class.as_str())
                && checker.current_method.as_deref() == Some("__construct")
                && (!span.identifies_a_node() || legacy_untyped_slot))
            && !internal_pdo_statement_initializer;
        // Public readonly overwrites raise their catchable Error before setter access matters.
        // Keep read visibility checks, and preserve the read-only branch of conditional writes.
        validate_object_property_access(
            checker, class_name, property, !(readonly_null_coalesce || readonly_write_guard), span,
        )?;
        if reflection_public_mirror_property(checker, class_name, property)
            && !compound_property_update
            && !is_compound_property_assignment_self_read(value, object, property, span, &[])
        {
            crate::types::checker::record_throw_access_site(
                &mut checker.throw_access_sites,
                checker.current_loop_storage_scope.clone(),
                span,
                crate::types::ThrowAccessInfo {
                    span,
                    kind: crate::types::ThrowAccessKind::ReflectionReadOnlyProperty {
                        property: property.to_string(),
                    },
                },
            );
            return Ok(());
        }
        if readonly_write_guard {
            let setter = class_info.property_set_visibilities.get(property)
                .or_else(|| class_info.property_visibilities.get(property))
                .cloned().unwrap_or(crate::parser::ast::Visibility::Public);
            let initialization_error = (!checker.can_access_member(&declaring_class, &setter))
                .then(|| {
                    let scope = checker.current_class.as_deref().map_or_else(
                        || "global scope".to_string(),
                        |scope| format!("scope {}", scope.trim_start_matches('\\')),
                    );
                    let readonly = if setter == crate::parser::ast::Visibility::Protected {
                        "readonly "
                    } else { "" };
                    format!("Cannot modify {}(set) {readonly}property {}::${property} from {scope}",
                        Checker::visibility_label(&setter), declaring_class.trim_start_matches('\\'))
                });
            let initialization_denied = initialization_error.is_some();
            // An initialized slot rejects an overwrite first. An uninitialized slot
            // instead honors its effective setter, including inherited protected access.
            crate::types::checker::record_throw_access_site(
                &mut checker.throw_access_sites,
                checker.current_loop_storage_scope.clone(),
                span,
                crate::types::ThrowAccessInfo {
                    span,
                    kind: crate::types::ThrowAccessKind::ReadonlyProperty {
                        class_name: declaring_class,
                        property: property.to_string(),
                        initialization_error,
                    },
                },
            );
            if initialization_denied { return Ok(()); }
        }
        // A property with a `get` hook but no `set` hook is read-only: external writes are an error
        // (PHP rejects writing a virtual/get-only hooked property). Writes from inside the property's
        // own accessor target the raw backing slot and are allowed.
        let has_get_hook = class_info
            .methods
            .contains_key(&php_symbol_key(&property_hook_get_method(property)));
        let has_set_hook = class_info
            .methods
            .contains_key(&php_symbol_key(&property_hook_set_method(property)));
        // `current_method` is stored as a lowercased symbol key, so compare against the lowercased
        // accessor names too — otherwise a mixed-case property (e.g. `$Total`) spuriously trips
        // the read-only check from inside its own accessor.
        let in_own_accessor = checker.current_class.as_deref() == Some(class_name)
            && checker.current_method.as_deref().is_some_and(|method| {
                method == php_symbol_key(&property_hook_get_method(property))
                    || method == php_symbol_key(&property_hook_set_method(property))
            });
        // A hooked property is only unwritable when it is VIRTUAL — no hook names `$this-><prop>`,
        // so there is no slot to write. A hook that does name it keeps ordinary storage, and PHP
        // writes that store directly (subject to `set` visibility, checked below). Symfony's
        // `ViewEvent::$controllerArgumentsEvent` declares a get hook that does
        // `$this->controllerArgumentsEvent ??= …` and the constructor assigns it.
        let is_backed = class_info.backed_hooked_properties.contains(property);
        if has_get_hook && !has_set_hook && !in_own_accessor && !is_backed {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot write to read-only hooked property {}::{} (it declares a get hook but no set hook)",
                    class_name, property
                ),
            ));
        }
        // A `mixed` value carries no compile-time type to compare with the declared slot, and
        // PHP checks exactly this assignment at run time under weak-mode property typing. The
        // codegen guard raises the catchable `TypeError` there, so rejecting the write here
        // would forbid a program PHP accepts whenever the runtime value happens to fit.
        // An initialized readonly slot throws before PHP considers the replacement type.
        // Its uninitialized branch still enforces that type at the runtime property store.
        let defers_to_runtime_property_typing = matches!(val_ty, PhpType::Mixed)
            || readonly_write_guard;
        if class_info.visible_property_is_declared(property) && !defers_to_runtime_property_typing {
            let accepts_stringable_object = !checker.strict_types
                && param_accepts_weak_string_coercion(&expected_ty)
                && matches!(val_ty.codegen_repr(), PhpType::Object(ref name) if checker.object_supports_weak_string_coercion(name));
            let defers_nominal_object_check =
                crate::types::param_binding::object_requires_runtime_nominal_guard(
                    &expected_ty,
                    val_ty,
                ) || crate::types::param_binding::gradual_object_requires_runtime_nominal_guard(
                    &expected_ty,
                    val_ty,
                );
            // A gradual value reaching a declared array property is PHP's RUNTIME check, not a
            // static one: `private array $options` accepts any array whatever its key shape, and
            // throws otherwise. The property boundary already lowers this through `MixedToHash`,
            // which tests the runtime tag and raises `TypeError` for a non-array, so refusing here
            // rejected programs `php -n` runs. Gated on the lowering's own predicate so the two
            // cannot disagree — accepting a target it does not convert would put an unconverted
            // `Mixed` in a concrete slot.
            let defers_gradual_array_check = matches!(
                val_ty.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) && crate::ir_lower::gradual_coercions::boundary_coercion_narrows_gradual(
                &expected_ty,
            );
            // A float stored into an `int` property is php's weak-mode coercion: an integral
            // value converts, a fractional one is a deprecation, and an out-of-range one a
            // `TypeError` -- decided on the value, so it cannot be refused on the type alone.
            // `$this->charCount += ($strlen - $ignoredL) / $this->fixedWidth` (symfony/mime
            // `CharacterStream`) divides two ints.
            let defers_weak_float_to_int = !checker.strict_types
                && expected_ty.codegen_repr() == PhpType::Int
                && matches!(val_ty.codegen_repr(), PhpType::Float);
            if !accepts_stringable_object
                && !defers_nominal_object_check
                && !defers_gradual_array_check
                && !defers_weak_float_to_int
            {
                checker.require_compatible_arg_type(
                    &expected_ty,
                    val_ty,
                    span,
                    &format!("Property {}::${}", class_name, property),
                )?;
            }
        }
    }
    Ok(())
}

/// Returns whether a declared property is one of PHP Reflection's direct-write-only mirrors.
///
/// These properties are not ordinary language-level `readonly` properties: PHP permits
/// `unset()`, reference binding, and compound mutation, but a direct assignment raises
/// `ReflectionException`. PHP applies the rule from the runtime object's Reflection ancestry,
/// even when a user subclass redeclares `name` or `class` itself.
fn reflection_public_mirror_property(
    checker: &Checker,
    class_name: &str,
    property: &str,
) -> bool {
    crate::types::reflection_properties::class_has_public_mirror_write_handler(
        &checker.classes, class_name, property,
    )
}

/// Validates that the current context can access a named property, checking visibility and class scope.
///
/// Looks up the property's declared class and visibility modifier, then delegates to
/// `Checker::can_access_member` to enforce access control rules.
fn validate_object_property_access(
    checker: &Checker,
    class_name: &str,
    property: &str,
    is_write: bool,
    span: Span,
) -> Result<(), CompileError> {
    let class_info = checker.classes.get(class_name).ok_or_else(|| {
        CompileError::new(span, &format!("Undefined class: {}", class_name))
    })?;
    // A write to a property with PHP 8.4 asymmetric visibility uses its `set` visibility; reads
    // and ordinary properties fall back to the regular (read) visibility.
    let asymmetric_write = if is_write {
        class_info.property_set_visibilities.get(property)
    } else {
        None
    };
    if let Some(visibility) =
        asymmetric_write.or_else(|| class_info.property_visibilities.get(property))
    {
        let declaring_class = class_info
            .property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name);
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot access {} property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
    }
    Ok(())
}

/// Returns `true` if `ty` is the empty-array placeholder `Array(Never)` produced by an `[]` literal.
///
/// Such a property has no known element type yet, so the first concrete array assignment should
/// adopt the assigned value's element type rather than stay pinned at `Never`.
fn is_empty_array_placeholder(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
}

/// Returns true when a value type's native property slot cannot also represent null,
/// so an untyped null-defaulted property storing it must use nullable-union storage.
/// Arrays are deliberately excluded: their pointer slots read a null pointer as PHP
/// null (var_dump and the null-container read paths recognize it), and keeping the
/// plain rebind preserves the GC-observable allocation profile — union storage would
/// box every stored array in a Mixed cell.
fn untyped_property_value_needs_nullable_storage(val_ty: &PhpType) -> bool {
    matches!(
        val_ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
    )
}

/// Computes the refined stored type for an untyped property after a write, shared by the
/// instance and static property refinement paths. Returns `None` when the type is unchanged.
///
/// Untyped properties start as `Void` (their default is null, implicit or explicit), so a
/// scalar/array value — whose native slot cannot also represent null — is stored as the
/// nullable `Union([Void, T])`, the same layout as a typed `?T` property, keeping the null
/// default observable. Object/Mixed and other pointer-shaped values keep the plain rebind:
/// their slots already encode null. A later write outside an existing nullable union widens
/// to `Mixed` (PHP untyped properties accept anything). Untyped non-null defaults keep the
/// historical rebind, and array-typed slots keep `[]`-placeholder adoption and generic-array
/// element specialization.
pub(super) fn refined_untyped_property_assignment_type(
    checker: &Checker,
    current: &PhpType,
    val_ty: &PhpType,
    definitely_initialized: bool,
) -> Option<PhpType> {
    match current {
        PhpType::Void => {
            if *val_ty == PhpType::Void {
                None
            } else if definitely_initialized {
                // Assignments inside the object's own constructor initialize the slot
                // before any observable read, so the historical precise rebind stays
                // (matching `propagate_constructor_arg_type` for promoted params).
                Some(val_ty.clone())
            } else if untyped_property_value_needs_nullable_storage(val_ty) {
                let stored = if matches!(val_ty, PhpType::False) {
                    PhpType::Bool
                } else {
                    val_ty.clone()
                };
                Some(PhpType::Union(vec![PhpType::Void, stored]))
            } else {
                Some(val_ty.clone())
            }
        }
        PhpType::Union(members)
            if members.len() == 2 && members[0] == PhpType::Void =>
        {
            if checker.type_accepts(current, val_ty) {
                None
            } else {
                Some(PhpType::Mixed)
            }
        }
        // An untyped slot holds whatever is written to it; a scalar default (`= 9`, `= false`,
        // `= ''`, `= 1.5`) is only its FIRST value, so a write of another kind widens it to
        // `Mixed`, the top of this lattice. Keeping the default's type made every other write a
        // silent coercion: Laravel-style `protected $withinTransaction = false;` assigned `'db'`
        // by a method read back as `true`. Rebinding the slot to the written type instead made
        // propinit store the `9` of `public $k = 9;` into a `string` slot, which failed the
        // build. Only the same scalar kind stays in place.
        PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float | PhpType::Str
            if !untyped_scalar_slot_holds(current, val_ty) =>
        {
            Some(PhpType::Mixed)
        }
        // A null-defaulted slot rebound to an object (or any other non-array pointer shape) by
        // its first write holds whatever is written next: an `int` written by a method was kept
        // as the object type, so the read of that slot was lowered as an object load with no way
        // to answer "unset" or 17. A null write stays, since the pointer slot already encodes it.
        _ if !current.is_php_array()
            && !matches!(current, PhpType::Mixed)
            && !matches!(val_ty, PhpType::Void | PhpType::Never)
            && !checker.type_accepts(current, val_ty) =>
        {
            Some(PhpType::Mixed)
        }
        _ => {
            if is_empty_array_placeholder(current)
                && matches!(val_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            {
                // An `[]` initializer types the property as `Array(Never)`; the
                // first concrete array assignment fixes the element type, exactly
                // as a plain local reassignment overwrites its type. Without this,
                // the element type stays `Never` and codegen mis-emits later reads.
                return Some(val_ty.clone());
            }
            if is_empty_array_placeholder(val_ty) && Checker::is_generic_array_hint(current) {
                // The converse: an `[]` written into an `array<mixed>` slot carries no element
                // evidence. Specializing toward it turned a `[1, 2]`-defaulted `static $cache`
                // into `array<never>`, and its default could no longer be stored. (Any other
                // element type still reaches the merge below, which widens it to mixed.)
                return None;
            }
            if let Some(widened) = untyped_array_storage_after_gradual_assignment(current, val_ty) {
                return Some(widened);
            }
            let refined_ty = Checker::specialize_generic_array_hint(current, val_ty);
            if refined_ty != *current {
                return Some(refined_ty);
            }
            merge_untyped_property_array_storage(current, val_ty)
        }
    }
}

/// Returns whether an untyped property slot of scalar storage `slot` represents every value of
/// `value` without conversion: the same scalar kind, or `false` into a `bool` slot.
fn untyped_scalar_slot_holds(slot: &PhpType, value: &PhpType) -> bool {
    match (slot, value) {
        (PhpType::Int, PhpType::Int) => true,
        (PhpType::Bool, PhpType::Bool | PhpType::False) => true,
        (PhpType::False, PhpType::False) => true,
        (PhpType::Float, PhpType::Float) => true,
        (PhpType::Str, PhpType::Str) => true,
        _ => false,
    }
}

/// Widens an UNTYPED array-shaped slot once a whole `Mixed`/union value is assigned to it.
///
/// An untyped property's storage shape is only ever a guess from the sites seen so far. An
/// initializer like `['ASCII', 'UTF-8']` types the slot `array<string>`, and another method then
/// assigns it the result of `array_map(...)`, which the checker knows only as `Mixed`. The
/// declared-`array` path already normalizes exactly this case
/// (`declared_array_storage_after_gradual_assignment`), but it may resolve to a hash because the
/// declaration GUARANTEES array-ness. An untyped property carries no such guarantee -- `self::$list
/// = 42;` is legal PHP -- so resolving to a hash would impose a `TypeError` PHP never raises. The
/// honest widening is `Mixed`, which is what an untyped PHP property actually holds.
///
/// Leaving the slot at `array<string>` is not an option either: codegen's store path widens the
/// assigned container's entries to boxed `Mixed` before the pointer enters the slot, so a slot
/// still claiming `array<string>` would have those entries read back as raw string pointers. The
/// refusal that fires today is therefore correct; this removes its cause rather than its message.
///
/// MONOTONE ON PURPOSE. `Mixed` is the top of this lattice and the first arm below never fires for
/// it, so a slot that reaches `Mixed` stays there. A rule that could narrow back is what made the
/// method-pass fixpoint run its whole budget once before, at 2.0s a pass.
fn untyped_array_storage_after_gradual_assignment(
    current: &PhpType,
    assigned: &PhpType,
) -> Option<PhpType> {
    if !matches!(assigned.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return None;
    }
    let element_is_already_mixed = match current.codegen_repr() {
        // An array whose entries are already boxed `Mixed` needs nothing: codegen's existing
        // unboxing store accepts it, and widening further would drop the array-ness fact for free.
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => return None,
    };
    (!element_is_already_mixed).then_some(PhpType::Mixed)
}

/// Widens incompatible array shapes written to one untyped property without inventing a PHP type.
///
/// Untyped properties frequently start as a precise associative shape after keyed writes and are
/// later replaced by a bare `array` parameter. Two associative shapes keep hash storage and widen
/// only their key/value facts; two indexed shapes widen to `array<mixed>`.
///
/// A cross-shape assignment resolves to the HASH, for the reason
/// `types::array_storage::join_array_storage_conversion` gives: a hash represents any PHP array,
/// a packed vector cannot. `array<mixed>` is NOT a runtime-dispatched "either" -- consumers read
/// the static type, so a hash parked in an `array<mixed>` slot is read as packed storage and
/// misreported (`json_encode` prints raw slot words, `in_array` faults). A list parked in a hash
/// costs nothing observable: measured against `php -n` 8.5.10, a hash holding sequential integer
/// keys encodes as `["a","b"]` and serializes as `a:2:{i:0;s:1:"a";i:1;s:1:"b";}`.
fn merge_untyped_property_array_storage(
    current: &PhpType,
    assigned: &PhpType,
) -> Option<PhpType> {
    match (current.codegen_repr(), assigned.codegen_repr()) {
        (
            PhpType::AssocArray {
                key: current_key,
                value: current_value,
            },
            PhpType::AssocArray {
                key: assigned_key,
                value: assigned_value,
            },
        ) => {
            let key = if current_key.codegen_repr() == assigned_key.codegen_repr() {
                current_key
            } else {
                Box::new(PhpType::Mixed)
            };
            let value = if current_value.codegen_repr() == assigned_value.codegen_repr() {
                current_value
            } else {
                Box::new(PhpType::Mixed)
            };
            let merged = PhpType::AssocArray { key, value };
            (merged != current.codegen_repr()).then_some(merged)
        }
        (PhpType::Array(current_element), PhpType::Array(assigned_element)) => {
            if current_element.codegen_repr() == assigned_element.codegen_repr() {
                None
            } else {
                Some(PhpType::Array(Box::new(PhpType::Mixed)))
            }
        }
        (
            PhpType::Array(_) | PhpType::AssocArray { .. },
            PhpType::Array(_) | PhpType::AssocArray { .. },
        ) => Some(PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        }),
        _ => None,
    }
}

/// Returns whether `property` is one of the untyped builtin Throwable fields with a fixed raw slot.
fn is_builtin_throwable_payload_property(declaring_class: &str, property: &str) -> bool {
    matches!(declaring_class.trim_start_matches('\\'), "Exception" | "Error")
        && matches!(property, "message" | "code")
}

/// Refines the inferred type of an object property after a write, for properties without declared types.
///
/// Delegates the type decision to `refined_untyped_property_assignment_type`; see its
/// documentation for the nullable-union storage rules. Only updates when the refined
/// type differs from the current type.
pub(super) fn refine_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
) {
    // A name php resolves to a DYNAMIC property never reaches the physical slot
    // `visible_property_index` finds for it, so widening that slot would restamp an ancestor's
    // private storage from a write that only ever creates a distinct dynamic property. The
    // ancestor keeps reading its own slot with its own type, so the stamp has to stay put.
    if scope_dynamic_storage::mutation_targets_scope_dynamic_name(checker, class_name, property) {
        return;
    }
    // A write inside the written class's own constructor (or a subclass constructor)
    // definitely initializes the slot before any observable read.
    let definitely_initialized = checker.current_method.as_deref() == Some("__construct")
        && checker.current_class.as_deref().is_some_and(|current| {
            current == class_name || checker.is_subclass_of(current, class_name)
        });
    let refined_ty = {
        let Some(class_info) = checker.classes.get(class_name) else {
            return;
        };
        if class_info.visible_property_is_declared(property) {
            return;
        }
        let Some(slot) = class_info.visible_property_index(property) else {
            return;
        };
        let Some(refined_ty) = refined_untyped_property_assignment_type(
            checker,
            &class_info.properties[slot].1,
            val_ty,
            definitely_initialized,
        ) else {
            return;
        };
        let declaring_class = class_info
            .property_declaring_classes
            .get(property)
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        // The builtin Throwable payload keeps `message` and `code` in a FIXED raw shape: the
        // native constructor, `getMessage()` and the uncaught-exception printer read a string
        // and an int there. Widening the slot made a subclass box its writes while the builtin
        // half kept reading raw words, so `$this->message = $this->rawMessage` -- Twig's
        // `Error::updateRepr()` -- left `getMessage()` answering "". The write is coerced to
        // the slot's shape instead, as for any typed property.
        if is_builtin_throwable_payload_property(&declaring_class, property) {
            return;
        }
        refined_ty
    };
    // Constructor writes can establish an array after the declaration pass (for example a null
    // default). Keep their layout identical to array-defaulted slots before any return reads it.
    let storage_ty = crate::types::ClassInfo::untyped_array_property_storage(refined_ty);
    update_inherited_property_slot_type(checker, class_name, property, storage_ty);
}

/// Keeps copies of one inherited property slot in the same storage shape.
///
/// Class layouts copy an ancestor's property metadata before method bodies are checked. A
/// later write can refine the ancestor's `Array(Never)` to an associative array, and codegen
/// must see that refinement on every subclass that inherited the same physical slot. A child
/// with a private property of the same name still carries the parent's slot, even though its
/// name lookup points at a separate slot. A non-private array redeclaration also shares the
/// slot, even when its default has already given it a different element type. All views of an
/// associative slot need one hash representation that can hold every redeclared default.
fn update_inherited_property_slot_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    ty: PhpType,
) {
    let Some(class_info) = checker.classes.get(class_name) else {
        return;
    };
    let Some(slot) = class_info.visible_property_index(property) else {
        return;
    };
    let old_ty = class_info.properties[slot].1.clone();
    if old_ty == ty && !matches!(ty, PhpType::AssocArray { .. }) {
        return;
    }
    let declaring_class = class_info
        .property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    let inheritors: Vec<String> = checker
        .classes
        .iter()
        .filter(|(name, info)| {
            (name.as_str() == declaring_class || checker.is_subclass_of(name, &declaring_class))
                && info.properties.get(slot).is_some_and(|(name, _)| name == property)
                && (info.visible_property_index(property) != Some(slot)
                    || info.property_declaring_classes.get(property) == Some(&declaring_class)
                    || info.properties[slot].1 == old_ty
                    || matches!(
                        (&ty, &info.properties[slot].1),
                        (PhpType::AssocArray { .. }, PhpType::Array(_) | PhpType::AssocArray { .. })
                    ))
        })
        .map(|(name, _)| name.clone())
        .collect();
    let shared_ty = if matches!(ty, PhpType::AssocArray { .. }) {
        inheritors.iter().fold(ty, |shared, name| {
            checker
                .classes
                .get(name)
                .map(|info| merge_associative_property_slot_type(checker, shared.clone(), &info.properties[slot].1))
                .unwrap_or(shared)
        })
    } else {
        ty
    };
    for name in inheritors {
        if let Some(info) = checker.classes.get_mut(&name) {
            info.properties[slot].1 = shared_ty.clone();
        }
    }
}

/// Joins a redeclared array default into the hash representation shared by one physical slot.
fn merge_associative_property_slot_type(
    checker: &Checker,
    shared: PhpType,
    candidate: &PhpType,
) -> PhpType {
    let PhpType::AssocArray { key, value } = shared else {
        return shared;
    };
    let (candidate_key, candidate_value) = match candidate {
        PhpType::Array(element) if matches!(element.as_ref(), PhpType::Never) => {
            return PhpType::AssocArray { key, value };
        }
        PhpType::Array(element) => (PhpType::Int, element.as_ref()),
        PhpType::AssocArray { key, value } => (*key.clone(), value.as_ref()),
        _ => return PhpType::AssocArray { key, value },
    };
    PhpType::AssocArray {
        key: Box::new(merge_array_key_types(*key, candidate_key)),
        value: Box::new(
            checker
                .merge_array_element_type(&value, candidate_value)
                .unwrap_or(PhpType::Mixed),
        ),
    }
}

/// Returns whether a value of `val_ty` can be stored into an extern class field of `field_ty`.
///
/// Integer fields of every width take an `int` (narrow widths keep its low bits, as C would);
/// float fields of either width take a `float` and also an `int`, converted like PHP's
/// int-to-float coercion; `bool` fields take a bool. Each of them also takes a value whose type
/// is only known at run time, which lowering narrows with the strict runtime check a packed
/// field of the same type gets (`fixed_layout_field_accepts_runtime_value`). Pointer fields
/// take any pointer or `null`; `string` fields take a string, copied into a fresh
/// null-terminated buffer the struct then owns.
fn extern_field_accepts(field_ty: &PhpType, val_ty: &PhpType) -> bool {
    if fixed_layout_field_accepts_runtime_value(field_ty, val_ty) {
        return true;
    }
    match field_ty {
        PhpType::Int => matches!(val_ty, PhpType::Int),
        PhpType::Float => matches!(val_ty, PhpType::Float | PhpType::Int),
        PhpType::Bool => matches!(val_ty, PhpType::Bool | PhpType::False),
        PhpType::Pointer(_) => matches!(val_ty, PhpType::Pointer(_) | PhpType::Void),
        PhpType::Str => matches!(val_ty, PhpType::Str),
        _ => false,
    }
}

/// Returns whether a fixed-layout scalar field of `field_ty` (a packed field, or an extern
/// struct field) admits a value of `val_ty` whose concrete type is only decided at run time.
///
/// Lowering narrows such a value strictly before the store (`Op::PackedFieldMixedToInt`,
/// `Op::PackedFieldMixedToFloat`, `Op::PackedFieldMixedToBool`): the field's own type passes,
/// an int widens into a float field, and anything else throws a catchable `TypeError`, so the
/// field never receives a box pointer or a silently coerced value. That is what lets arithmetic
/// with an open result type feed these fields without a false compile error: integer
/// arithmetic is `mixed` or `int|float` for its overflow-to-float promotion, and float
/// arithmetic over a loop counter is `mixed` as well. `int|float` is the precise spelling of
/// the same open result, so refusing it would make the narrower type the stricter one
/// (`$i = 0; $i++; $thing->id = $i;` refused where `$i = $i + 1` is accepted). A `bool` field
/// takes `mixed` but not `int|float`, which never holds a bool. A pointer field takes no
/// runtime value: a pointer boxes under the int tag, so no runtime check could tell an address
/// from an arbitrary integer.
fn fixed_layout_field_accepts_runtime_value(field_ty: &PhpType, val_ty: &PhpType) -> bool {
    match field_ty {
        PhpType::Int | PhpType::Float => {
            matches!(val_ty, PhpType::Mixed) || val_ty.is_int_float_union()
        }
        PhpType::Bool => matches!(val_ty, PhpType::Mixed),
        _ => false,
    }
}

/// Validates a write to a property of a typed pointer (extern or packed class).
///
/// Checks that the field exists in `extern_field_type` or `packed_field_type` and that the
/// assigned value's type is compatible with the field's declared type.
fn check_pointer_property_write(
    checker: &Checker,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if let Some(field_ty) = checker.extern_field_type(class_name, property) {
        if !extern_field_accepts(&field_ty, val_ty) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {:?} to extern field {}::{} of type {:?}",
                    val_ty, class_name, property, field_ty
                ),
            ));
        }
    } else if let Some(PhpType::Packed(inner)) = checker.packed_field_type(class_name, property) {
        // A nested record field is assigned as a whole by copying the source record's bytes,
        // so the value must be the address of a record of the SAME packed class: a buffer
        // element, another nested field, or a `ptr<Inner>`.
        let same_record = matches!(
            val_ty,
            PhpType::Pointer(Some(source)) | PhpType::Packed(source)
                if source.trim_start_matches('\\').eq_ignore_ascii_case(inner.trim_start_matches('\\'))
        );
        if !same_record {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {} to packed field {}::{} of type {}; a nested packed field takes a {} record",
                    val_ty, class_name, property, inner, inner
                ),
            ));
        }
    } else if let Some(field_ty) = checker.packed_field_type(class_name, property) {
        // A value whose type is only known at run time is admitted into a scalar field because
        // lowering narrows it strictly before the store (see
        // `fixed_layout_field_accepts_runtime_value`).
        if &field_ty != val_ty
            && !matches!((&field_ty, val_ty), (PhpType::Bool, PhpType::False))
            && !fixed_layout_field_accepts_runtime_value(&field_ty, val_ty)
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {} to packed field {}::{} of type {}",
                    val_ty, class_name, property, field_ty
                ),
            ));
        }
    } else if checker.extern_classes.contains_key(class_name) {
        return Err(CompileError::new(
            span,
            &format!("Undefined extern field: {}::{}", class_name, property),
        ));
    } else if checker.packed_classes.contains_key(class_name) {
        return Err(CompileError::new(
            span,
            &format!("Undefined packed field: {}::{}", class_name, property),
        ));
    }
    Ok(())
}

/// Returns whether `property` is a slot this class simply does not declare but still accepts.
///
/// `check_object_property_write` already lets a plain `$o->newProp = v` through for exactly these
/// classes; an element write (`$o->newProp[] = v`, `$o->newProp[$k] = v`) is the same write with
/// an index on it and has to agree. It did not: `(object) ['vars' => []]` gives a stdClass whose
/// keys the compiler never sees, and Symfony's `CompiledUrlMatcherDumper` — which builds its whole
/// working state that way and then does `$state->vars[] = $m[1]` — was rejected with
/// "Undefined property: stdClass::vars".
///
/// Magic `__set` is deliberately NOT included: PHP answers an element write on an overloaded
/// property with "Indirect modification of overloaded property has no effect", so accepting it
/// here would promise something the language does not do.
fn object_property_write_is_dynamic(checker: &Checker, class_name: &str, property: &str) -> bool {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return true;
    }
    checker
        .classes
        .get(class_name)
        .is_some_and(|class_info| {
            class_info.allow_dynamic_properties && class_info.visible_property(property).is_none()
        })
}

/// Resolves the type of a named property on a class object, for array property operations.
///
/// Looks up the property in `checker.classes`, validates existence and access, and returns
/// the property type along with a flag indicating whether the property has a declared type.
/// Returns an error if the class or property is undefined.
/// Whether `class_name` names an interface (and no class): a property write through it is
/// resolved on the run-time object, as for an untyped object.
fn receiver_is_interface_only(checker: &Checker, class_name: &str) -> bool {
    let name = class_name.trim_start_matches('\\');
    !checker.classes.contains_key(name) && checker.interfaces.contains_key(name)
}

fn resolve_object_array_property(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
) -> Result<(PhpType, bool), CompileError> {
    let class_info = checker
        .classes
        .get(class_name)
        .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", class_name)))?;
    if class_info.visible_property(property).is_none() {
        return Err(CompileError::new(
            span,
            &format!("Undefined property: {}::{}", class_name, property),
        ));
    }
    // Indirect array modification (`$obj->prop[] = x` / `$obj->prop[$k] = x`) is a write, so it
    // must honor PHP 8.4 asymmetric `set` visibility — not the read visibility.
    validate_object_property_access(checker, class_name, property, true, span)?;
    let property_has_declared_type = class_info.visible_property_is_declared(property);
    let prop_ty = class_info
        .visible_property(property)
        .map(|(_, (_, ty))| ty.clone())
        .unwrap_or(PhpType::Int);
    Ok((prop_ty, property_has_declared_type))
}

/// Computes the updated type of an array property after a push operation (`$prop[] = value`).
///
/// For typed arrays: validates the pushed value against the element type via `require_compatible_arg_type`.
/// For untyped arrays: merges the pushed value's type into the element type via `merge_array_element_type`.
/// For `AssocArray` properties: gradual-merges the appended integer key and value because a PHP
/// `array` declaration does not constrain either dimension.
/// For nullable/false array unions: accepts PHP's null/false auto-vivification and preserves the
/// declared union storage shape.
/// For untyped `Int` or `Void` base types: converts the property to `array<value_type>`.
/// Returns an error for buffer types or non-array property types.
fn updated_array_property_push_type(
    checker: &Checker,
    prop_ty: &PhpType,
    property_has_declared_type: bool,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    if prop_ty.is_php_array() {
        return Ok(prop_ty.clone());
    }
    match prop_ty {
        PhpType::Array(elem_ty) => {
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
                Ok(PhpType::Array(elem_ty.clone()))
            } else if elem_ty.as_ref() == val_ty {
                Ok(PhpType::Array(elem_ty.clone()))
            } else {
                let merged_ty = checker
                    .merge_array_element_type(elem_ty, val_ty)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
        }
        PhpType::Int | PhpType::Void if !property_has_declared_type => {
            Ok(PhpType::Array(Box::new(val_ty.clone())))
        }
        PhpType::Buffer(_) => Err(CompileError::new(
            span,
            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
        )),
        // The boxed storage type is preserved and the runtime writer performs the mutation and
        // PHP's auto-vivification, exactly as `updated_array_property_assign_type` already does
        // for the KEYED write. Only the push arm was missing, so `$this->brackets[] = $x` against
        // an untyped `private $brackets;` was refused where `$this->brackets[$k] = $x` was not.
        // Twig's `Lexer` writes it five times.
        PhpType::Mixed => Ok(prop_ty.clone()),
        PhpType::AssocArray { key, value } => {
            let merged_value = checker
                .merge_array_element_type(value, val_ty)
                .unwrap_or(PhpType::Mixed);
            let merged_key = checker
                .merge_array_element_type(key, &PhpType::Int)
                .unwrap_or(PhpType::Mixed);
            Ok(PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            })
        }
        PhpType::Union(members) if array_family_bool_void_union_accepts_write(members) => {
            Ok(prop_ty.clone())
        }
        // `protected array|string $sqlStatements` (Doctrine's SQL executors) holds an array where
        // it is pushed to; the boxed writer appends to it, and php's `[] operator not supported
        // for strings` Error stays the run-time answer for a string.
        PhpType::Union(members)
            if members.iter().any(|member| matches!(member, PhpType::Str))
                && array_family_bool_void_union_accepts_write(
                    &members
                        .iter()
                        .filter(|member| !matches!(member, PhpType::Str))
                        .cloned()
                        .collect::<Vec<_>>(),
                ) =>
        {
            Ok(prop_ty.clone())
        }
        other => Err(CompileError::new(
            span,
            &format!("Array push requires an array property, got {}", other),
        )),
    }
}

/// Computes the updated type of an array property after an indexed write (`$prop[$index] = value`).
///
/// Handles `PhpType::Array` and `PhpType::AssocArray` storage:
/// - For `Array`: validates element type compatibility, handles `Never`-element arrays specially
///   (converts to `AssocArray` when a static key forces hash storage), and merges element types
///   for untyped properties.
/// - For `AssocArray`: merges the key type with the index type and merges the value type with
///   the assigned value, preserving declared-type constraints.
/// - For `Mixed` and nullable/false array unions: preserves the boxed storage type while the
///   runtime writer performs the mutation and PHP-compatible auto-vivification.
fn updated_array_property_assign_type(
    checker: &Checker,
    prop_ty: &PhpType,
    property_has_declared_type: bool,
    class_name: &str,
    property: &str,
    index: &Expr,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    if prop_ty.is_php_array() {
        return Ok(prop_ty.clone());
    }
    match prop_ty {
        PhpType::Array(elem_ty) => {
            if !matches!(normalized_idx_ty, PhpType::Int)
                || (matches!(elem_ty.as_ref(), PhpType::Never)
                    && static_array_key_forces_hash_storage(index))
            {
                if property_has_declared_type {
                    checker.require_compatible_arg_type(
                        elem_ty.as_ref(),
                        val_ty,
                        span,
                        &format!("Property {}::${}[]", class_name, property),
                    )?;
                }
                return Ok(assoc_property_type_after_keyed_write(
                    checker,
                    elem_ty,
                    property_has_declared_type,
                    normalized_idx_ty,
                    val_ty,
                ));
            }
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
                Ok(PhpType::Array(elem_ty.clone()))
            } else if elem_ty.as_ref() == val_ty {
                Ok(PhpType::Array(elem_ty.clone()))
            } else {
                let merged_ty = checker
                    .merge_array_element_type(elem_ty, val_ty)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
        }
        PhpType::AssocArray {
            key,
            value: existing_value,
        } => {
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    existing_value.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
            }
            let merged_key = merge_array_key_types(*key.clone(), normalized_idx_ty.clone());
            let merged_value = if property_has_declared_type || existing_value.as_ref() == val_ty {
                *existing_value.clone()
            } else {
                checker
                    .merge_array_element_type(existing_value, val_ty)
                    .unwrap_or(PhpType::Mixed)
            };
            Ok(PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            })
        }
        PhpType::Mixed => Ok(prop_ty.clone()),
        // `$this->types[$param - 1] = 'i'` on a `string` property (Doctrine DBAL's mysqli
        // `Statement`) is php's byte-offset write; the lowering writes the string back.
        PhpType::Str if matches!(normalized_idx_ty, PhpType::Int | PhpType::Mixed) => {
            Ok(PhpType::Str)
        }
        PhpType::Union(members) if array_family_bool_void_union_accepts_write(members) => {
            Ok(prop_ty.clone())
        }
        // A union holding an array among other shapes is a boxed cell, written through the same
        // run-time dispatch as a `mixed` property: an array takes the element, an `ArrayAccess`
        // object gets `offsetSet()`, anything else raises php's Error. symfony/http-kernel's
        // `DataCollector::$data` is `array|Data`, and every collector writes `$this->data[...]`.
        PhpType::Union(members)
            if prop_ty.codegen_repr() == PhpType::Mixed
                && members
                    .iter()
                    .any(|member| matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. })) =>
        {
            Ok(prop_ty.clone())
        }
        other => Err(CompileError::new(
            span,
            &format!(
                "Array index assignment requires an array property, got {}",
                other
            ),
        )),
    }
}

/// Returns true when `members` contains at least one array storage member and all remaining
/// alternatives are PHP's null/false auto-vivification cases.
pub(super) fn array_family_bool_void_union_accepts_write(members: &[PhpType]) -> bool {
    let mut saw_array = false;
    for member in members {
        match member {
            // An `iterable` member makes the union a boxed cell too (`string|iterable|null
            // $body`, Symfony's `MockResponse`): the run-time writer appends to an array and
            // raises php's Error for a `Traversable` that is not `ArrayAccess`.
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
                saw_array = true
            }
            PhpType::Bool | PhpType::False | PhpType::Void => {}
            _ => return false,
        }
    }
    saw_array
}

/// Returns true if `ty` can be coerced to a PHP array key for a write operation.
pub(super) fn is_php_array_key_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int
        | PhpType::Str
        | PhpType::Mixed
        | PhpType::Bool
        | PhpType::False
        | PhpType::Float
        | PhpType::Void
        | PhpType::Never => true,
        PhpType::Union(members) => members.iter().all(is_php_array_key_type),
        _ => false,
    }
}

/// Returns whether every non-null member is an object implementing `ArrayAccess`.
pub(super) fn type_satisfies_array_access(checker: &Checker, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class_name) => {
            checker.object_type_implements_interface(class_name, "ArrayAccess")
        }
        PhpType::Union(members) => {
            let mut saw_array_access = false;
            for member in members {
                match member {
                    PhpType::Void | PhpType::Never => {}
                    PhpType::Object(class_name)
                        if checker.object_type_implements_interface(class_name, "ArrayAccess") =>
                    {
                        saw_array_access = true;
                    }
                    _ => return false,
                }
            }
            saw_array_access
        }
        _ => false,
    }
}

/// Computes the resulting `PhpType::AssocArray` type after writing to an array property with a
/// non-integer or static-computed key.
///
/// Special-cases `PhpType::Never` element types (treats the key and value as derived from the
/// write operands). Otherwise merges the element type with the assigned value type via
/// `merge_array_element_type`. If the property has a declared `Mixed` element type, returns
/// a fully `Mixed` `AssocArray` to preserve type soundness.
pub(super) fn assoc_property_type_after_keyed_write(
    checker: &Checker,
    elem_ty: &PhpType,
    property_has_declared_type: bool,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
) -> PhpType {
    if property_has_declared_type && matches!(elem_ty, PhpType::Mixed) {
        return PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        };
    }

    let merged_key = if matches!(elem_ty, PhpType::Never) {
        normalized_idx_ty.clone()
    } else {
        merge_array_key_types(PhpType::Int, normalized_idx_ty.clone())
    };
    let merged_value = if matches!(elem_ty, PhpType::Never) {
        val_ty.clone()
    } else if elem_ty == val_ty {
        elem_ty.clone()
    } else {
        checker
            .merge_array_element_type(elem_ty, val_ty)
            .unwrap_or(PhpType::Mixed)
    };
    PhpType::AssocArray {
        key: Box::new(merged_key),
        value: Box::new(merged_value),
    }
}

/// Updates the in-memory type of an object property after an array operation.
///
/// Writes the updated type back to `checker.classes` only if the property has no declared type,
/// or if the update satisfies `declared_generic_array_can_use_assoc_storage` (permits converting
/// a `Mixed` element array to `AssocArray` storage when appropriate).
fn update_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    property_has_declared_type: bool,
    updated_prop_ty: PhpType,
) {
    let Some(class_info) = checker.classes.get(class_name) else {
        return;
    };
    let Some((_, (_, current_type))) = class_info.visible_property(property) else {
        return;
    };
    let current_type = current_type.clone();
    // An INFERRED property's stored type may only widen. Overwriting it let one pass narrow what
    // an earlier pass had widened, and the method-pass fixpoint then alternated between the two
    // forever instead of settling. The join is also the honest answer: the slot has to hold every
    // value any site assigns to it.
    let updated_prop_ty = if property_has_declared_type {
        updated_prop_ty
    } else {
        checker
            .merged_assignment_type(&current_type, &updated_prop_ty)
            .unwrap_or(PhpType::Mixed)
    };
    if !property_has_declared_type
        || declared_generic_array_can_use_assoc_storage(&current_type, &updated_prop_ty)
    {
        update_inherited_property_slot_type(checker, class_name, property, updated_prop_ty);
    }
}

/// Returns true if a declared generic array property may be updated to `AssocArray` storage.
///
/// Currently returns true only when the property is declared as `array<PhpType::Mixed>` and the
/// updated type is an `AssocArray` with `PhpType::Mixed` values. This guards against widening
/// a typed array to an associative storage with a narrower element type.
pub(super) fn declared_generic_array_can_use_assoc_storage(
    current: &PhpType,
    updated: &PhpType,
) -> bool {
    matches!(
        (current, updated),
        (
            PhpType::Array(elem_ty),
            PhpType::AssocArray { key: _, value }
        ) if matches!(elem_ty.as_ref(), PhpType::Mixed)
            && matches!(value.as_ref(), PhpType::Mixed)
    )
}

/// Resolves the type of a field on a typed pointer (extern class or packed struct) for array operations.
///
/// Checks `extern_field_type` then `packed_field_type`. Returns the field type if found.
/// If the class is known as extern/packed but the field is not defined, returns an error
/// describing the undefined field. The `operation` string is used only in error messages.
fn resolve_pointer_field_type(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
    operation: &str,
) -> Result<PhpType, CompileError> {
    if let Some(field_ty) = checker.extern_field_type(class_name, property) {
        Ok(field_ty)
    } else if let Some(field_ty) = checker.packed_field_type(class_name, property) {
        Ok(field_ty)
    } else if checker.extern_classes.contains_key(class_name) {
        Err(CompileError::new(
            span,
            &format!("Undefined extern field: {}::{}", class_name, property),
        ))
    } else if checker.packed_classes.contains_key(class_name) {
        Err(CompileError::new(
            span,
            &format!("Undefined packed field: {}::{}", class_name, property),
        ))
    } else {
        Err(CompileError::new(
            span,
            &format!("{} requires an object or typed pointer", operation),
        ))
    }
}
