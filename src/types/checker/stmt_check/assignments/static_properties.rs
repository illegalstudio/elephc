//! Purpose:
//! Type-checks assignment static properties forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::span::Span;
use crate::types::{
    normalized_array_key_type, static_array_key_forces_hash_storage, PhpType, TypeEnv,
};

use super::super::super::Checker;
use super::properties::{
    array_family_bool_void_union_accepts_write, assoc_property_type_after_keyed_write,
    is_php_array_key_type, type_satisfies_array_access,
};

/// Internal data for static property assignment resolution.
/// Holds the resolved class, declaring class, declared-type status, and current property type.
struct StaticPropertyAssignmentTarget {
    class_name: String,
    declaring_class: String,
    property_has_declared_type: bool,
    prop_ty: PhpType,
    dynamic_eval_target: bool,
}

/// Gives an inferred static array a boxed PHP contract before nested write-context traversal.
/// Concrete child reads otherwise detach boxes, and homogeneous children reject legal writes.
pub(super) fn promote_static_nested_write_root(
    checker: &mut Checker, target: &Expr, span: Span,
) -> Result<(), CompileError> {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let ExprKind::StaticPropertyAccess { receiver, property } = &root.kind else { return Ok(()); };
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    if !target.property_has_declared_type
        && matches!(target.prop_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        update_static_property_type(checker, property, &target.declaring_class, PhpType::php_array());
    }
    Ok(())
}

/// Type-checks a direct static property assignment `Class::$prop = value`.
///
/// Infers the value type, resolves the property target via `resolve_static_property_assignment_target`,
/// validates type compatibility against declared types, and refines the property type when no
/// declared type is present.
pub(super) fn check_static_property_assign(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    record_static_property_callable_sig(checker, &target.declaring_class, property, value, env)?;

    if target.property_has_declared_type {
        checker.require_compatible_arg_type(
            &target.prop_ty,
            &val_ty,
            span,
            &format!("Static property {}::${}", target.class_name, property),
        )?;
        if let Some(updated_ty) =
            declared_array_storage_after_gradual_assignment(&target.prop_ty, &val_ty)
        {
            update_static_property_type(
                checker,
                property,
                &target.declaring_class,
                updated_ty,
            );
        }
    }

    if !target.property_has_declared_type {
        refine_static_property_assignment_type(
            checker,
            property,
            &target.declaring_class,
            &val_ty,
        );
    }
    Ok(())
}

/// Widens a declared PHP `array` slot after a whole-value gradual assignment.
///
/// A default value may specialize the compiler's storage metadata to an indexed array or a
/// typed associative hash, but PHP's declared `array` contract does not constrain keys or element
/// values. Once a `Mixed`/union value is assigned, normalize the whole-program slot metadata to a
/// Mixed-keyed/Mixed-valued hash so codegen can validate array-ness and copy either runtime array
/// representation without subsequently reading its elements through a stale specialized type.
fn declared_array_storage_after_gradual_assignment(
    current: &PhpType,
    assigned: &PhpType,
) -> Option<PhpType> {
    if !matches!(assigned.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return None;
    }
    if !matches!(
        current.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    ) {
        return None;
    }
    Some(PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    })
}

/// Type-checks an array-push assignment `Class::$prop[] = value`.
///
/// Infers the value type, resolves the property target, validates element-type compatibility
/// against declared types, merges element types when the property is untyped, and updates the
/// property type. Rejects buffer properties and non-array static properties.
pub(super) fn check_static_property_array_push(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    if target.dynamic_eval_target || target.prop_ty.is_php_array()
        || matches!(&target.prop_ty, PhpType::Object(name) if name.eq_ignore_ascii_case("WeakMap"))
    {
        return Ok(());
    }
    let updated_prop_ty = match target.prop_ty {
        PhpType::Array(elem_ty) => {
            if target.property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    &val_ty,
                    span,
                    &format!("Static property {}::${}[]", target.class_name, property),
                )?;
                PhpType::Array(elem_ty)
            } else if *elem_ty == val_ty {
                PhpType::Array(elem_ty)
            } else {
                let merged_ty = checker
                    .merge_array_element_type(&elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed);
                PhpType::Array(Box::new(merged_ty))
            }
        }
        PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
            PhpType::Array(Box::new(val_ty.clone()))
        }
        // A keyed write earlier (`static::$map[$name] = $v`) made the slot a hash, and php appends
        // to a hash exactly as to a list: the next integer key. The instance-property push already
        // merges this way (`updated_array_property_push_type`); the static one refused it, which
        // is how `Env::extend()` — a named OR an appended adapter — refused its whole class file.
        PhpType::AssocArray { key, value } => {
            let merged_value = checker
                .merge_array_element_type(&value, &val_ty)
                .unwrap_or(PhpType::Mixed);
            let merged_key = checker
                .merge_array_element_type(&key, &PhpType::Int)
                .unwrap_or(PhpType::Mixed);
            PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            }
        }
        PhpType::Mixed => PhpType::Mixed,
        // A declared `?array` (PHPUnit's `ExtensionSettings::$settings`): php appends to the array,
        // or starts one from null.
        PhpType::Union(members)
            if target.property_has_declared_type
                && members.iter().all(|member| {
                    matches!(
                        member,
                        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Void
                    )
                }) =>
        {
            return Ok(());
        }
        PhpType::Buffer(_) => {
            return Err(CompileError::new(
                span,
                "buffer<T> does not support push; allocate with buffer_new<T>(len)",
            ))
        }
        other => {
            return Err(CompileError::new(
                span,
                &format!("Array push requires an array static property, got {}", other),
            ))
        }
    };

    if !target.property_has_declared_type {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            updated_prop_ty,
        );
    }
    Ok(())
}

/// Type-checks an indexed array assignment `Class::$prop[index] = value`.
///
/// Infers the index and value types, resolves the property target, validates PHP array keys,
/// validates element-type compatibility against declared types, merges element types when the
/// property is untyped, and updates the property type. Short-circuits for `ArrayAccess` objects.
/// A `buffer<T>` property takes the whole-element write rules of a local or instance-property
/// buffer and keeps its type.
pub(super) fn check_static_property_array_assign(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    let mixed_elements = match target.prop_ty.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    };
    if target.dynamic_eval_target || target.prop_ty.is_php_array() || mixed_elements {
        if !is_dynamic_eval_static_property_array_key_type(&normalized_idx_ty) {
            return Err(CompileError::new(span, "Array index must be integer"));
        }
        return Ok(());
    }
    if type_satisfies_array_access(checker, &target.prop_ty) {
        return Ok(());
    }
    if let PhpType::Buffer(elem_ty) = &target.prop_ty {
        super::arrays::validate_buffer_element_write(elem_ty, &idx_ty, &val_ty, span)?;
        return Ok(());
    }
    if !is_php_array_key_type(&normalized_idx_ty) {
        return Err(CompileError::new(span, "Array index must be integer"));
    }

    let declared_prop_ty = target.prop_ty.clone();
    let updated_prop_ty = match target.prop_ty {
        PhpType::Array(elem_ty) => {
            if target.property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    &val_ty,
                    span,
                    &format!("Static property {}::${}[]", target.class_name, property),
                )?;
            }
            // A key that is not an integer — or a static key that forces hash storage into a
            // still-empty (`Never`-element) array — makes this container associative, exactly as
            // it does for an instance property. PHP has one array type: an empty `[]` becomes a
            // hash the moment a string key lands in it. Promoting the static slot here is the
            // same program-wide de-pack the reference-alias slice performs through
            // `update_static_property_type`, so codegen loads/stores/frees it as a hash and the
            // write lowers to `hash_set` instead of the integer-key-only `array_set`.
            if !matches!(normalized_idx_ty, PhpType::Int)
                || (matches!(elem_ty.as_ref(), PhpType::Never)
                    && static_array_key_forces_hash_storage(index))
            {
                assoc_property_type_after_keyed_write(
                    checker,
                    &elem_ty,
                    target.property_has_declared_type,
                    &normalized_idx_ty,
                    &val_ty,
                )
            } else if target.property_has_declared_type || *elem_ty == val_ty {
                PhpType::Array(elem_ty)
            } else {
                let merged_ty = checker
                    .merge_array_element_type(&elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed);
                PhpType::Array(Box::new(merged_ty))
            }
        }
        PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
            PhpType::Array(Box::new(val_ty.clone()))
        }
        PhpType::AssocArray { key, value } => {
            // A string/int/mixed-key write into an associative static array — including one
            // de-packed to a hash by a reference-alias (`self::$a[$dir] = &self::$a[$k]`). Keep it
            // associative and widen the value type toward the written value when untyped.
            let merged_value = if target.property_has_declared_type {
                checker.require_compatible_arg_type(
                    value.as_ref(),
                    &val_ty,
                    span,
                    &format!("Static property {}::${}[]", target.class_name, property),
                )?;
                *value
            } else if *value == val_ty {
                *value
            } else {
                checker
                    .merge_array_element_type(&value, &val_ty)
                    .unwrap_or(PhpType::Mixed)
            };
            PhpType::AssocArray {
                key,
                value: Box::new(merged_value),
            }
        }
        PhpType::Union(members) if array_family_bool_void_union_accepts_write(&members) => {
            // PHP auto-vivifies a nullable/false array-valued static property on indexed write.
            // Keep the declared union storage shape; EIR mutates its boxed cell in place through
            // the same mixed-array runtime helper used for instance properties.
            PhpType::Union(members)
        }
        // An untyped slot left `null` by its default and written with values of several kinds is
        // a boxed `mixed`; php auto-vivifies the array on the first keyed write
        // (`static::$resolvedInstance[$name] = $instance`), and the lowering does the same through
        // the mixed-array writer, publishing the box back to the slot.
        PhpType::Mixed => PhpType::Mixed,
        other => {
            return Err(CompileError::new(
                span,
                &format!(
                    "Array index assignment requires an array static property, got {}",
                    other
                ),
            ))
        }
    };

    // A property declared as the unconstrained PHP `array` may still be re-storaged as a hash: a
    // bare `array` hint says nothing about keys, so `AssocArray<_, Mixed>` is the same contract in
    // a representation the string-key write can actually use. Any narrower declared element type
    // keeps its packed storage (`declared_generic_array_can_use_assoc_storage` refuses it), which
    // is the same guard instance properties use.
    if !target.property_has_declared_type
        || super::properties::declared_generic_array_can_use_assoc_storage(
            &declared_prop_ty,
            &updated_prop_ty,
        )
    {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            updated_prop_ty,
        );
    }
    Ok(())
}

/// Type-checks a write through a dynamic-named static property (`self::${$n} = v`,
/// `self::${$n}[$k] = v`, `self::${$n}[] = v`).
///
/// The property name is a runtime string, so no single candidate can be pinpointed: the receiver
/// class must be statically known (else a loud deferred error, as in the read path) and must
/// declare at least one static property. Operands are inferred in source order (name, index,
/// value) for their assignment effects. Value/element type checking is intentionally permissive
/// (gradual): since the runtime name selects among candidates, individual candidate types are not
/// narrowed here — codegen picks and coerces the matching one.
pub(super) fn check_dynamic_static_property_write(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &Expr,
    index: Option<&Expr>,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let name_ty = checker.infer_type_with_assignment_effects(property, env)?;
    if !matches!(name_ty, PhpType::Str | PhpType::Mixed | PhpType::Int) {
        return Err(CompileError::new(
            property.span,
            &format!(
                "Dynamic static property name must be a string, got `{}`",
                name_ty
            ),
        ));
    }
    if let Some(index) = index {
        checker.infer_type_with_assignment_effects(index, env)?;
    }
    checker.infer_type_with_assignment_effects(value, env)?;

    let class_name = resolve_dynamic_static_write_class(checker, receiver, span)?;
    let class_info = checker.classes.get(&class_name).ok_or_else(|| {
        CompileError::new(
            span,
            "Dynamic static property access requires a statically-known class",
        )
    })?;
    if class_info.static_properties.is_empty() {
        return Err(CompileError::new(
            span,
            &format!("Class {} has no static properties", class_name),
        ));
    }
    Ok(())
}

/// Resolves the receiver of a dynamic static property write to a statically-known class name.
///
/// `Named` returns the class directly; `Self_`/`Static` require a class context; `Parent` returns
/// the current class's parent. Errors when the class is not statically resolvable so the write
/// stays a loud deferred error rather than a silent miscompile.
fn resolve_dynamic_static_write_class(
    checker: &Checker,
    receiver: &StaticReceiver,
    span: Span,
) -> Result<String, CompileError> {
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(class_name) => Ok(class_name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                CompileError::new(
                    span,
                    "Dynamic static property access requires a statically-known class",
                )
            }),
        StaticReceiver::Parent => {
            let current_class = checker.current_class.as_ref().ok_or_else(|| {
                CompileError::new(
                    span,
                    "Dynamic static property access requires a statically-known class",
                )
            })?;
            checker
                .classes
                .get(current_class)
                .and_then(|info| info.parent.clone())
                .ok_or_else(|| {
                    CompileError::new(
                        span,
                        &format!("Class {} has no parent class", current_class),
                    )
                })
        }
    }
}

/// Resolves `receiver` to a class name and fetches static property metadata.
///
/// Returns `StaticPropertyAssignmentTarget` with class name, declaring class,
/// declared-type flag, and current property type. Checks that the property exists
/// and that the current context can access it per PHP visibility rules.
/// Records the callable signature of a closure stored in a STATIC property, keyed so a later
/// `(self::$prop)(…)` call site can find its by-reference parameters.
///
/// A closure in a static property is a program-wide singleton. It may be assigned once and invoked
/// later as `(self::$callback)($first, $second, $output, …)`, where a parameter can be BY
/// REFERENCE and therefore DEFINES `$expiredIds` in the caller. Without a recorded signature the
/// call site cannot know that, and the caller's later `if ($expiredIds)` reads an undefined
/// variable.
///
/// `??=` is unwrapped here rather than in `resolve_expr_callable_sig`: the parser lowers
/// `self::$p ??= <closure>` to a null-coalesce whose LEFT branch is the target property itself, and
/// the general resolver requires BOTH branches to agree. For a self-referential `??=` the null case
/// means "not assigned yet", so the signature is the DEFAULT's alone. Narrowing the unwrap to this
/// exact shape keeps the shared branch-matching rule untouched.
fn record_static_property_callable_sig(
    checker: &mut Checker,
    declaring_class: &str,
    property: &str,
    value: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let source = match &value.kind {
        ExprKind::NullCoalesce { value: current, default }
            if matches!(
                &current.kind,
                ExprKind::StaticPropertyAccess { property: current_property, .. }
                    if current_property == property
            ) =>
        {
            default.as_ref()
        }
        _ => value,
    };
    // `\Closure::bind(<closure>, $newThis, $scope)` / `\Closure::fromCallable(<closure>)` keep the
    // wrapped closure's parameters — rebinding `$this` or a scope changes neither the parameter
    // list nor which parameters are by-reference. The wrapper therefore has to be seen through
    // for a bound closure's by-reference parameters to register.
    //
    // ⚠️ The signature is read STRUCTURALLY here, never through `resolve_expr_callable_sig`.
    // Resolving a closure node CHECKS ITS BODY, and a `Closure::bind` body is written against the
    // scope it is about to be bound INTO — re-checking it here, outside that scope, turns the very
    // protected accesses `Closure::bind` exists to grant into errors. Routing this through the
    // general resolver was measured at ledger 68 -> 98 (ten `CacheItem::$expiry`/`$key` access
    // errors plus cascades) before being backed out.
    let bound = closure_behind_bind_wrapper(source).unwrap_or(source);
    if let Some(sig) = structural_closure_by_ref_sig(bound) {
        checker
            .static_property_callable_sigs
            .insert(format!("{}::${}", declaring_class, property), sig);
        return Ok(());
    }
    let Some(sig) = checker.resolve_expr_callable_sig(source, env)? else {
        return Ok(());
    };
    checker
        .static_property_callable_sigs
        .insert(format!("{}::${}", declaring_class, property), sig);
    Ok(())
}

/// Unwraps `\Closure::bind(<expr>, …)` / `\Closure::fromCallable(<expr>)` to their first argument.
///
/// Only a NAMED `Closure` receiver qualifies: `self::`/`static::`/`parent::` inside `Closure` is
/// not a shape user code writes, and treating them as `Closure` would misread an unrelated class.
fn closure_behind_bind_wrapper(expr: &Expr) -> Option<&Expr> {
    let ExprKind::StaticMethodCall { receiver, method, args, .. } = &expr.kind else {
        return None;
    };
    let StaticReceiver::Named(name) = receiver else {
        return None;
    };
    if crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')) != "closure" {
        return None;
    }
    if !matches!(
        crate::names::php_symbol_key(method).as_str(),
        "bind" | "fromcallable"
    ) {
        return None;
    }
    args.first()
}

/// Builds a by-reference-only signature for a closure LITERAL, straight from its parameter list.
///
/// This reads the AST and nothing else: no body check, no type-hint resolution, no diagnostics —
/// which is the whole point (see the caller). It carries just what
/// `sig_undefined_by_ref_variable_outputs` consults: `ref_params`, and `declared_params` left all
/// false so every defined variable is inserted as `Mixed`.
///
/// Returns `None` when the expression is not a closure literal, when it has no by-reference
/// parameter (nothing to record), or when a by-reference parameter carries a TYPE HINT — that last
/// case would need the declared type inserted verbatim so call validation and the caller's storage
/// agree, and deriving it needs the resolver this function deliberately avoids.
fn structural_closure_by_ref_sig(expr: &Expr) -> Option<crate::types::FunctionSig> {
    let ExprKind::Closure { params, variadic, variadic_by_ref, .. } = &expr.kind else {
        return None;
    };
    let mut names = Vec::with_capacity(params.len());
    let mut ref_params = Vec::with_capacity(params.len());
    for (name, type_ann, _default, is_ref) in params {
        if *is_ref && type_ann.is_some() {
            return None;
        }
        names.push((name.clone(), PhpType::Mixed));
        ref_params.push(*is_ref);
    }
    if let Some(name) = variadic {
        names.push((name.clone(), PhpType::Array(Box::new(PhpType::Mixed))));
        ref_params.push(*variadic_by_ref);
    }
    if !ref_params.iter().any(|is_ref| *is_ref) {
        return None;
    }
    let count = names.len();
    Some(crate::types::FunctionSig {
        params: names,
        param_type_exprs: vec![None; count],
        param_attributes: Vec::new(),
        defaults: vec![None; count],
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params,
        declared_params: vec![false; count],
        variadic: variadic.clone(),
        deprecation: None,
        is_generator: false,
    })
}

fn resolve_static_property_assignment_target(
    checker: &Checker,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> Result<StaticPropertyAssignmentTarget, CompileError> {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    let class_name = match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| CompileError::new(span, "Cannot use self:: outside class method scope"))?,
        StaticReceiver::Static => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| CompileError::new(span, "Cannot use static:: outside class method scope"))?,
        StaticReceiver::Parent => {
            let current_class = checker.current_class.as_ref().ok_or_else(|| {
                CompileError::new(span, "Cannot use parent:: outside class method scope")
            })?;
            let current_info = checker.classes.get(current_class).ok_or_else(|| {
                CompileError::new(span, &format!("Undefined class: {}", current_class))
            })?;
            current_info.parent.as_ref().cloned().ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!("Class {} has no parent class", current_class),
                )
            })?
        }
    };

    if checker.eval_barrier_active
        && matches!(receiver, StaticReceiver::Named(_))
        && !checker.classes.contains_key(&class_name)
    {
        return Ok(StaticPropertyAssignmentTarget {
            class_name: class_name.clone(),
            declaring_class: class_name,
            property_has_declared_type: false,
            prop_ty: PhpType::Mixed,
            dynamic_eval_target: true,
        });
    }

    let class_info = checker
        .classes
        .get(&class_name)
        .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", class_name)))?;
    if !class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return Err(CompileError::new(
            span,
            &format!("Undefined static property: {}::{}", class_name, property),
        ));
    }
    if let Some(visibility) = class_info.static_property_visibilities.get(property) {
        let declaring_class = class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot access {} static property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
    }
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let property_has_declared_type = class_info.declared_static_properties.contains(property);
    let prop_ty = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int);

    Ok(StaticPropertyAssignmentTarget {
        class_name,
        declaring_class,
        property_has_declared_type,
        prop_ty,
        dynamic_eval_target: false,
    })
}

/// Returns true when a dynamic eval static-property key can be handled by runtime array writes.
fn is_dynamic_eval_static_property_array_key_type(ty: &PhpType) -> bool {
    // A union key travels boxed exactly like `mixed`: `self::$fileOffsets[$file]` with `$file`
    // from `ReflectionMethod::getFileName()` (`string|false`) in Symfony's DebugClassLoader.
    matches!(ty, PhpType::Int | PhpType::Str | PhpType::Mixed)
        || (matches!(ty, PhpType::Union(_)) && ty.codegen_repr() == PhpType::Mixed)
}

/// Updates the type of a static property in all classes that declare it.
///
/// Iterates over all classes and mutates the type entry for `property` on the
/// declaring class `declaring_class`. Used after array-push or index assignment
/// to propagate the new element type.
fn update_static_property_type(
    checker: &mut Checker,
    property: &str,
    declaring_class: &str,
    updated_ty: PhpType,
) {
    for class_info in checker.classes.values_mut() {
        if class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            != Some(declaring_class)
        {
            continue;
        }
        if let Some(prop) = class_info
            .static_properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            prop.1 = updated_ty.clone();
        }
    }
}

/// Type-checks `self::$a[$dir] = &self::$a[$k]`: aliasing one element of a static-property array to
/// an element of a static-property array (SLICE 2/3).
///
/// The `receiver::property` target element becomes a reference alias of the source element's shared
/// cell. Both operands must be array-typed static properties whose element fits in a single
/// reference-cell word (an untyped `array`/`Mixed`-element array, or an already-associative array).
/// The target — and the source when it names a different property — is de-packed program-wide from
/// an indexed `array` to the promoted associative hash type via `update_static_property_type`, so
/// codegen loads/stores/frees it as a hash (the static analog of retyping a local Array→AssocArray).
///
/// Every out-of-scope shape is a loud error rather than a silent value copy: an unknown/`Mixed`
/// class or undefined/inaccessible property (from `resolve_static_property_assignment_target`), a
/// non-array static property, a string element, a typed indexed-list container, or a reference
/// source that is not itself a static-property array element.
pub(super) fn check_ref_assign_static_prop_element(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    source: &Expr,
    span: Span,
) -> Result<(), CompileError> {
    // Resolve + validate the TARGET static property. Unknown/`Mixed` class and undefined/
    // inaccessible property all surface here as loud errors.
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    validate_reference_array_static_property(&target.prop_ty, span)?;

    // For this slice the reference SOURCE must itself be a static-property array element
    // (`&self::$SRC[$k]`); any other shape is a follow-up slice, not a silent value copy.
    let ExprKind::ArrayAccess {
        array: src_array, ..
    } = &source.kind
    else {
        return Err(reference_source_shape_error(span));
    };
    let ExprKind::StaticPropertyAccess {
        receiver: src_receiver,
        property: src_property,
    } = &src_array.kind
    else {
        return Err(reference_source_shape_error(span));
    };
    let src_target =
        resolve_static_property_assignment_target(checker, src_receiver, src_property, span)?;
    validate_reference_array_static_property(&src_target.prop_ty, span)?;

    // This slice covers only the same-array GATE (`self::$a[$d] = &self::$a[$k]`), where both
    // operands are the SAME static property. A cross-array source (`self::$a[$d] = &self::$b[$k]`)
    // needs a separate source load/store round-trip and is a follow-up slice: loud-error it rather
    // than miscompile.
    if (src_target.declaring_class.as_str(), src_property.as_str())
        != (target.declaring_class.as_str(), property)
    {
        return Err(CompileError::new(
            span,
            "Reference between two different static-property arrays is not yet supported (source must alias the same static property)",
        ));
    }

    // De-pack signal: flip the (single, shared) target/source static property from an indexed
    // `array` to the promoted associative hash so codegen routes reads/writes/cleanup as a hash
    // program-wide.
    let target_assoc = promoted_static_array_type(&target.prop_ty);
    update_static_property_type(checker, property, &target.declaring_class, target_assoc);
    Ok(())
}

/// Prepares a static-property array element as the source of a local reference alias.
///
/// The static slot is promoted to associative storage because PHP references require a stable
/// managed cell and the source key may be either an integer or a string. The returned type is the
/// value stored inside that cell and becomes the type of the aliasing local.
pub(super) fn check_local_ref_assign_static_prop_element(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<PhpType, CompileError> {
    let index_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_index_ty = normalized_array_key_type(index, index_ty);
    if !is_php_array_key_type(&normalized_index_ty) {
        return Err(CompileError::new(span, "Invalid associative array key type"));
    }
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    validate_reference_array_static_property(&target.prop_ty, span)?;
    let element_ty = static_array_element_type(&target.prop_ty);
    let assoc_ty = promoted_static_array_type(&target.prop_ty);
    update_static_property_type(checker, property, &target.declaring_class, assoc_ty);
    Ok(element_ty)
}

/// Validates that a static property targeted by a reference-into-element alias is an array whose
/// element fits in a single reference-cell word.
///
/// Rejects (loud errors): a non-array static property, a string element (multi-word — a one-word
/// cell would drop its length), and a concretely-typed indexed scalar list whose packed
/// representation must stay list-shaped. An untyped `array`/`Mixed`-element array and an already
/// associative array are accepted.
fn validate_reference_array_static_property(
    prop_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    // A declared `array` is the boxed packed-or-hash union; it is promoted to a Mixed-valued hash
    // like an untyped `array<mixed>` one.
    if prop_ty.is_php_array() {
        return Ok(());
    }
    let repr = prop_ty.codegen_repr();
    if !matches!(repr, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            span,
            "Reference assignment into a static-property array element requires an array static property",
        ));
    }
    let element = static_array_element_type(prop_ty);
    if element.codegen_repr() == PhpType::Str {
        return Err(CompileError::new(
            span,
            "Reference into a string-element static-property array is not yet supported",
        ));
    }
    if matches!(repr, PhpType::Array(_))
        && matches!(
            element.codegen_repr(),
            PhpType::Int | PhpType::Float | PhpType::Bool
        )
    {
        return Err(CompileError::new(
            span,
            "unsupported: reference into a typed indexed-list static property",
        ));
    }
    Ok(())
}

/// Returns the element type of an array/associative container (defaulting to `Mixed`).
fn static_array_element_type(container: &PhpType) -> PhpType {
    let element = match container.codegen_repr() {
        PhpType::AssocArray { value, .. } => *value,
        PhpType::Array(element) => *element,
        _ => PhpType::Mixed,
    };
    if element == PhpType::Never {
        PhpType::Mixed
    } else {
        element
    }
}

/// Builds the promoted associative-hash type an indexed/`array` static property is retyped to when
/// one of its elements is reference-aliased.
fn promoted_static_array_type(container: &PhpType) -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(static_array_element_type(container)),
    }
}

/// Builds the loud error for a reference source that is not itself a static-property array element.
fn reference_source_shape_error(span: Span) -> CompileError {
    CompileError::new(
        span,
        "Reference source for a static-property array element must be another static-property array element (e.g. &self::$a[$k])",
    )
}

/// Refines the type of a static property based on an assigned value.
///
/// Delegates the type decision to `refined_untyped_property_assignment_type` (shared with
/// instance properties), which stores scalar/array writes to untyped null-defaulted slots
/// as nullable unions so the null default stays observable. Only updates when the refined
/// type differs from the current.
fn refine_static_property_assignment_type(
    checker: &mut Checker,
    property: &str,
    declaring_class: &str,
    val_ty: &PhpType,
) {
    let refined_ty = {
        let current = checker.classes.values().find_map(|class_info| {
            if class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                != Some(declaring_class)
            {
                return None;
            }
            class_info
                .static_properties
                .iter()
                .find(|(name, _)| name == property)
                .map(|(_, ty)| ty.clone())
        });
        let Some(current) = current else {
            return;
        };
        let Some(refined_ty) = super::properties::refined_untyped_property_assignment_type(
            checker, &current, val_ty, false,
        ) else {
            return;
        };
        refined_ty
    };
    for class_info in checker.classes.values_mut() {
        if class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            != Some(declaring_class)
        {
            continue;
        }
        if let Some(prop) = class_info
            .static_properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            prop.1 = refined_ty.clone();
        }
    }
}
