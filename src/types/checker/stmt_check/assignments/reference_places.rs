//! Purpose:
//! Type-checks `<place> = &$source;` where the place is an object property or an array element.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments` (`StmtKind::RefAssignPlace`).
//!
//! Key details:
//! - The parser guarantees a plain-variable source (other sources are first bound to a hidden
//!   local with `RefAssign`). An undefined source is created as null, like PHP's `&$v`.
//! - A property place is validated as an ordinary write of the source (visibility, readonly,
//!   declared type) and recorded for reference-property promotion, so every access to it goes
//!   through the shared cell. An UNTYPED property is widened to boxed storage (PHP `array`
//!   when both sides are arrays, else `mixed`), because a reference can carry any value into
//!   it. The source local takes the property's type from here on.
//! - An element place joins a boxed `Mixed` hash entry, so it is validated as a write of a
//!   `Mixed` value (widening the container's element type) and the source local is retyped to
//!   `Mixed`: a write through the element can store any type into it.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;
use super::{arrays, locals, properties};

/// Type-checks one `RefAssignPlace` statement.
pub(super) fn check_ref_assign_place(
    checker: &mut Checker,
    target: &Expr,
    source: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let ExprKind::Variable(source_name) = &source.kind else {
        return Err(CompileError::new(
            span,
            "Reference assignment source must be a variable, array/property element, or a by-reference call",
        ));
    };
    if checker.active_statics.contains(source_name) {
        return Err(CompileError::new(
            span,
            &format!(
                "Binding the static local ${source_name} into a property or array element by \
                 reference is not supported yet: its storage has no reference cell the place \
                 could share"
            ),
        ));
    }
    if let Some(index) = checker.current_by_ref_param_index(source_name) {
        checker.escaping_ref_params.insert((source_name.clone(), index));
    }
    if !env.contains_key(source_name) {
        // `$a[$k] = &$v` with no prior `$v` creates it as null, without an undefined warning.
        env.insert(source_name.clone(), PhpType::Mixed);
    }
    checker.invalidate_boxed_ref_alias_binding(source_name);
    checker.ref_aliased_locals.insert(source_name.clone());
    checker.active_ref_params.insert(source_name.clone());
    checker.record_reference_alias_root(target);
    if let Some((object, name)) = dynamic_place_root(target) {
        return check_dynamic_ref_place(checker, target, object, name, source_name, span, env);
    }
    match &target.kind {
        ExprKind::PropertyAccess { object, property } => {
            properties::check_property_assign(checker, object, property, source, span, env)?;
            let object_ty = checker.infer_type(object, env)?;
            let Some(class) = crate::types::checker::single_object_class_name(&object_ty) else {
                return Err(CompileError::new(
                    span,
                    "Reference assignment to a property requires a receiver whose class is known at compile time",
                ));
            };
            // An untyped property has no representation of its own to keep: a reference can
            // carry any value into it from either side, so its slot becomes boxed (`array` when
            // both sides are arrays, `mixed` otherwise). A declared property keeps its declared
            // storage and the source must agree with it.
            let source_ty = env.get(source_name).cloned().unwrap_or(PhpType::Mixed);
            properties::widen_referenced_untyped_property(checker, &class, property, &source_ty);
            checker
                .reference_property_promotions
                .insert((class, property.clone()));
            let property_ty = checker.infer_type(target, env)?;
            env.insert(source_name.clone(), property_ty);
            Ok(())
        }
        ExprKind::ArrayAccess { array, index } => {
            let probe = crate::names::generated_local_name("__elephc_ref_place_probe");
            env.insert(probe.clone(), PhpType::Mixed);
            let value = Expr::new(ExprKind::Variable(probe.clone()), source.span);
            let result = match &array.kind {
                ExprKind::Variable(root) => {
                    arrays::check_array_assign(checker, root, index, &value, span, env)
                }
                ExprKind::PropertyAccess { object, property } => {
                    properties::check_property_array_assign(
                        checker, object, property, index, &value, span, env,
                    )
                }
                // A nested place (`$a['k']['n'] = &$v`) is reached exactly like the nested
                // source of `$r = &$a['k']['n']`: every level is made addressable through its
                // own managed entry cell, so it is validated by the same rule.
                _ => {
                    let place_probe =
                        crate::names::generated_local_name("__elephc_ref_place_target_probe");
                    let checked = locals::check_ref_assign(checker, &place_probe, target, span, env);
                    env.remove(&place_probe);
                    checked
                }
            };
            env.remove(&probe);
            result?;
            // An element place reached through a property (`$this->p['k'] = &$v`) is made
            // addressable through that property's reference cell, like `$r = &$this->p['k']`.
            if let Some((object, property)) = element_place_property_root(array) {
                let object_ty = checker.infer_type(object, env)?;
                if let Some(class) = crate::types::checker::single_object_class_name(&object_ty) {
                    // The receiver alias reads the property's cell as boxed storage, so an
                    // untyped array property becomes PHP `array` (one boxed representation).
                    properties::widen_referenced_untyped_property(
                        checker,
                        &class,
                        property,
                        &PhpType::php_array(),
                    );
                    checker
                        .reference_property_promotions
                        .insert((class, property.to_string()));
                }
            }
            env.insert(source_name.clone(), PhpType::Mixed);
            Ok(())
        }
        _ => Err(CompileError::new(
            span,
            "Reference assignment target must be a variable, an object property or an array element",
        )),
    }
}

/// Returns the runtime-named property (`$o->{$k}`) a place is, or that its element chain starts
/// from (`$o->{$k}['x']`), if any.
pub(crate) fn dynamic_place_root(place: &Expr) -> Option<(&Expr, &Expr)> {
    match &place.kind {
        ExprKind::DynamicPropertyAccess { object, property } => Some((object, property)),
        ExprKind::ArrayAccess { array, .. } => dynamic_place_root(array),
        _ => None,
    }
}

/// Type-checks `$o->{$k} = &$v`, `$o->{$k}['x'] = &$v` and `$o->{$k}[] = &$v` (desugared to an
/// element place).
///
/// The name is only known at run time, so every property it can select must be able to share the
/// reference: each untyped property of the receiver's class and subclasses is widened to boxed
/// storage and promoted to a reference property, exactly as a statically named bind does, and a
/// typed property whose storage is already boxed is promoted too. Any other typed property is
/// refused per name at run time by the lowering. A name that selects no declared property lands
/// in the object's property hash (a `stdClass` property or a dynamic property), so the receiver
/// subtree records the hash reservation an ordinary runtime-name write records.
fn check_dynamic_ref_place(
    checker: &mut Checker,
    target: &Expr,
    object: &Expr,
    name: &Expr,
    source_name: &str,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let object_ty = checker.infer_type(object, env)?;
    if !matches!(object_ty, PhpType::Object(_) | PhpType::Union(_) | PhpType::Mixed) {
        return Err(CompileError::new(span, "Property assignment requires an object"));
    }
    let name_ty = checker.infer_type(name, env)?;
    if !matches!(name_ty, PhpType::Str | PhpType::Int | PhpType::Mixed) {
        return Err(CompileError::new(
            name.span,
            "Dynamic property name must be string or integer",
        ));
    }
    if let ExprKind::ArrayAccess { index, .. } = &target.kind {
        checker.infer_type(index, env)?;
    }
    let direct = matches!(target.kind, ExprKind::DynamicPropertyAccess { .. });
    promote_runtime_name_candidates(checker, &object_ty, direct);
    env.insert(source_name.to_string(), PhpType::Mixed);
    Ok(())
}

/// Type-checks the source of `$r = &$o->{$k}`: the receiver and the name, and the promotion of
/// every property the name can select, exactly as for a runtime-named place.
pub(crate) fn check_dynamic_ref_source(
    checker: &mut Checker,
    object: &Expr,
    name: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let object_ty = checker.infer_type(object, env)?;
    if !matches!(object_ty, PhpType::Object(_) | PhpType::Union(_) | PhpType::Mixed) {
        return Err(CompileError::new(span, "Property assignment requires an object"));
    }
    let name_ty = checker.infer_type(name, env)?;
    if !matches!(name_ty, PhpType::Str | PhpType::Int | PhpType::Mixed) {
        return Err(CompileError::new(
            name.span,
            "Dynamic property name must be string or integer",
        ));
    }
    promote_runtime_name_candidates(checker, &object_ty, true);
    Ok(())
}

/// Records the property-hash reservation for a runtime-named reference and promotes every
/// property of the receiver's class subtree that can share the reference (`direct`: the
/// property itself; otherwise an element of it).
fn promote_runtime_name_candidates(checker: &mut Checker, object_ty: &PhpType, direct: bool) {
    crate::types::checker::scope_dynamic_storage::record_scope_dynamic_runtime_name_receiver_mutation(
        checker,
        object_ty,
    );
    let receiver_class = match object_ty {
        PhpType::Object(class) => Some(class.trim_start_matches('\\').to_string()),
        _ => checker.union_single_object_class(object_ty),
    };
    if let Some(class) = receiver_class {
        let subtree: Vec<String> = checker
            .classes
            .keys()
            .filter(|candidate| {
                candidate.as_str() == class
                    || crate::types::class_inherits_from(&checker.classes, candidate, &class)
            })
            .cloned()
            .collect();
        for owner in subtree {
            let names: Vec<(String, bool, PhpType)> = match checker.classes.get(&owner) {
                Some(info) => info
                    .properties
                    .iter()
                    .map(|(property, ty)| {
                        (property.clone(), info.visible_property_is_declared(property), ty.clone())
                    })
                    .collect(),
                None => continue,
            };
            for (property, declared, ty) in names {
                if !declared {
                    let shape = if direct { PhpType::Mixed } else { PhpType::php_array() };
                    properties::widen_referenced_untyped_property(checker, &owner, &property, &shape);
                } else {
                    let repr = ty.codegen_repr();
                    let shares = repr == PhpType::Mixed || (!direct && ty.is_php_array());
                    if !shares {
                        continue;
                    }
                }
                checker
                    .reference_property_promotions
                    .insert((owner.clone(), property));
            }
        }
    }
}

/// Returns the property an element place's container chain starts from, if any.
pub(super) fn element_place_property_root(container: &Expr) -> Option<(&Expr, &str)> {
    match &container.kind {
        ExprKind::PropertyAccess { object, property } => Some((object, property)),
        ExprKind::ArrayAccess { array, .. } => element_place_property_root(array),
        _ => None,
    }
}

impl Checker {
    /// Returns the position of `name` among the by-reference parameters of the function or
    /// method whose body is being checked.
    fn current_by_ref_param_index(&self, name: &str) -> Option<usize> {
        let sig = match (&self.current_class, &self.current_method) {
            (Some(class), Some(method)) => self.classes.get(class)?.methods.get(method)?,
            _ => self.functions.get(self.current_function.as_deref()?)?,
        };
        let index = sig.params.iter().position(|(param, _)| param == name)?;
        sig.ref_params.get(index).copied().unwrap_or(false).then_some(index)
    }
}
