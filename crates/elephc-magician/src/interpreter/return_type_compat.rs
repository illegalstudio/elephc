//! Purpose:
//! Validates method type compatibility for eval-declared method signatures.
//! This keeps class/interface parameter contravariance and return covariance
//! checks out of the statement dispatcher.
//!
//! Called from:
//! - `crate::interpreter::statements` while registering eval class-like declarations.
//!
//! Key details:
//! - `self`, `parent`, and `static` are resolved relative to the declaration owner.
//! - Parameter checks reverse the return-type relation because PHP parameters are contravariant.
//! - Pending class declarations are checked before they are registered in the eval context.

use super::*;

/// Returns whether an implementation can accept every required declared parameter type.
pub(super) fn method_parameter_type_signature_accepts(
    implementation_types: &[Option<EvalParameterType>],
    implementation_variadics: &[bool],
    implementation_owner: &str,
    required_types: &[Option<EvalParameterType>],
    required_variadics: &[bool],
    required_owner: &str,
    required_param_count: usize,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    (0..required_param_count).all(|position| {
        let implementation_type = method_signature_effective_parameter_type(
            implementation_types,
            implementation_variadics,
            position,
        );
        let required_type = method_signature_effective_parameter_type(
            required_types,
            required_variadics,
            position,
        );
        eval_parameter_type_accepts(
            implementation_type,
            implementation_owner,
            required_type,
            required_owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether a method preserves the required declared return type contract.
pub(super) fn method_return_type_signature_accepts(
    implementation_type: Option<&EvalParameterType>,
    implementation_owner: &str,
    required_type: Option<&EvalParameterType>,
    required_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    let Some(required_type) = required_type else {
        return true;
    };
    implementation_type.is_some_and(|implementation_type| {
        eval_return_type_accepts(
            required_type,
            required_owner,
            implementation_type,
            implementation_owner,
            pending_class,
            context,
        )
    })
}

/// Returns the parameter type that applies at one source-order argument position.
fn method_signature_effective_parameter_type<'a>(
    parameter_types: &'a [Option<EvalParameterType>],
    variadics: &[bool],
    position: usize,
) -> Option<&'a EvalParameterType> {
    if let Some(variadic_index) = variadics.iter().position(|is_variadic| *is_variadic) {
        if position >= variadic_index {
            return parameter_types
                .get(variadic_index)
                .and_then(Option::as_ref);
        }
    }
    parameter_types.get(position).and_then(Option::as_ref)
}

/// Returns whether an implementation parameter type is a PHP contravariant supertype.
fn eval_parameter_type_accepts(
    implementation_type: Option<&EvalParameterType>,
    implementation_owner: &str,
    required_type: Option<&EvalParameterType>,
    required_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    match (implementation_type, required_type) {
        (None, _) => true,
        (Some(implementation_type), None) => eval_type_is_mixed(implementation_type),
        (Some(implementation_type), Some(required_type)) => eval_return_type_accepts(
            implementation_type,
            implementation_owner,
            required_type,
            required_owner,
            pending_class,
            context,
        ),
    }
}

/// Returns whether `actual_type` is a covariant subtype of `expected_type`.
fn eval_return_type_accepts(
    expected_type: &EvalParameterType,
    expected_owner: &str,
    actual_type: &EvalParameterType,
    actual_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if eval_return_type_is_never(actual_type) {
        return true;
    }
    if eval_return_type_is_never(expected_type) {
        return false;
    }
    if eval_return_type_is_void(expected_type) {
        return eval_return_type_is_void(actual_type);
    }
    if eval_return_type_is_void(actual_type) {
        return false;
    }
    if eval_return_type_allows_null(actual_type) && !eval_return_type_allows_null(expected_type) {
        return false;
    }
    if actual_type.variants().is_empty() {
        return eval_return_type_allows_null(expected_type);
    }
    if expected_type.variants().is_empty() {
        return false;
    }
    if actual_type.is_intersection() {
        return eval_return_type_accepts_actual_intersection(
            expected_type,
            expected_owner,
            actual_type,
            actual_owner,
            pending_class,
            context,
        );
    }
    actual_type.variants().iter().all(|actual_variant| {
        eval_return_type_accepts_actual_variant(
            expected_type,
            expected_owner,
            actual_variant,
            actual_owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether a return type can produce PHP null, including standalone `mixed`.
fn eval_return_type_allows_null(return_type: &EvalParameterType) -> bool {
    return_type.allows_null()
        || eval_type_is_mixed(return_type)
}

/// Returns whether one retained type is PHP's standalone `mixed` atom.
fn eval_type_is_mixed(parameter_type: &EvalParameterType) -> bool {
    !parameter_type.is_intersection()
        && matches!(parameter_type.variants(), [EvalParameterTypeVariant::Mixed])
}

/// Returns whether a return type is exactly PHP `never`.
fn eval_return_type_is_never(return_type: &EvalParameterType) -> bool {
    !return_type.allows_null()
        && !return_type.is_intersection()
        && matches!(return_type.variants(), [EvalParameterTypeVariant::Never])
}

/// Returns whether a return type is exactly PHP `void`.
fn eval_return_type_is_void(return_type: &EvalParameterType) -> bool {
    !return_type.allows_null()
        && !return_type.is_intersection()
        && matches!(return_type.variants(), [EvalParameterTypeVariant::Void])
}

/// Returns whether an expected type accepts an actual intersection return type.
fn eval_return_type_accepts_actual_intersection(
    expected_type: &EvalParameterType,
    expected_owner: &str,
    actual_type: &EvalParameterType,
    actual_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if expected_type.is_intersection() {
        return expected_type.variants().iter().all(|expected_variant| {
            eval_return_variant_accepts_actual_intersection(
                expected_variant,
                expected_owner,
                actual_type,
                actual_owner,
                pending_class,
                context,
            )
        });
    }
    expected_type.variants().iter().any(|expected_variant| {
        eval_return_variant_accepts_actual_intersection(
            expected_variant,
            expected_owner,
            actual_type,
            actual_owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether an expected type accepts one actual non-intersection atom.
fn eval_return_type_accepts_actual_variant(
    expected_type: &EvalParameterType,
    expected_owner: &str,
    actual_variant: &EvalParameterTypeVariant,
    actual_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if expected_type.is_intersection() {
        return expected_type.variants().iter().all(|expected_variant| {
            eval_return_variant_accepts_actual_variant(
                expected_variant,
                expected_owner,
                actual_variant,
                actual_owner,
                pending_class,
                context,
            )
        });
    }
    expected_type.variants().iter().any(|expected_variant| {
        eval_return_variant_accepts_actual_variant(
            expected_variant,
            expected_owner,
            actual_variant,
            actual_owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether one expected atom accepts an actual intersection return type.
fn eval_return_variant_accepts_actual_intersection(
    expected_variant: &EvalParameterTypeVariant,
    expected_owner: &str,
    actual_type: &EvalParameterType,
    actual_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    actual_type.variants().iter().any(|actual_variant| {
        eval_return_variant_accepts_actual_variant(
            expected_variant,
            expected_owner,
            actual_variant,
            actual_owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether one expected non-null atom accepts one actual non-null atom.
fn eval_return_variant_accepts_actual_variant(
    expected_variant: &EvalParameterTypeVariant,
    expected_owner: &str,
    actual_variant: &EvalParameterTypeVariant,
    actual_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    match (expected_variant, actual_variant) {
        (_, EvalParameterTypeVariant::Never) => true,
        (EvalParameterTypeVariant::Never, _) => false,
        (EvalParameterTypeVariant::Void, _) | (_, EvalParameterTypeVariant::Void) => false,
        (EvalParameterTypeVariant::Mixed, _) => true,
        (EvalParameterTypeVariant::Object, EvalParameterTypeVariant::Object)
        | (EvalParameterTypeVariant::Array, EvalParameterTypeVariant::Array)
        | (EvalParameterTypeVariant::Bool, EvalParameterTypeVariant::Bool)
        | (EvalParameterTypeVariant::Callable, EvalParameterTypeVariant::Callable)
        | (EvalParameterTypeVariant::Float, EvalParameterTypeVariant::Float)
        | (EvalParameterTypeVariant::Int, EvalParameterTypeVariant::Int)
        | (EvalParameterTypeVariant::Iterable, EvalParameterTypeVariant::Iterable)
        | (EvalParameterTypeVariant::String, EvalParameterTypeVariant::String) => true,
        (EvalParameterTypeVariant::Object, EvalParameterTypeVariant::Class(_)) => true,
        (EvalParameterTypeVariant::Iterable, EvalParameterTypeVariant::Array) => true,
        (EvalParameterTypeVariant::Iterable, EvalParameterTypeVariant::Class(actual_name)) => {
            eval_return_class_type_is_a(
                actual_name,
                actual_owner,
                "Traversable",
                actual_owner,
                pending_class,
                context,
            ) || eval_return_class_type_is_a(
                actual_name,
                actual_owner,
                "Iterator",
                actual_owner,
                pending_class,
                context,
            )
        }
        (
            EvalParameterTypeVariant::Class(expected_name),
            EvalParameterTypeVariant::Class(actual_name),
        ) => eval_return_class_type_accepts(
            expected_name,
            expected_owner,
            actual_name,
            actual_owner,
            pending_class,
            context,
        ),
        _ => false,
    }
}

/// Returns whether one declared class-like return atom is covariant with another.
fn eval_return_class_type_accepts(
    expected_name: &str,
    expected_owner: &str,
    actual_name: &str,
    actual_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if expected_name.eq_ignore_ascii_case("static") {
        return actual_name.eq_ignore_ascii_case("static");
    }
    if actual_name.eq_ignore_ascii_case("static") {
        return eval_return_class_type_is_a(
            actual_owner,
            actual_owner,
            expected_name,
            expected_owner,
            pending_class,
            context,
        );
    }
    let Some(actual_resolved) =
        eval_resolve_return_class_type_name(actual_name, actual_owner, pending_class, context)
    else {
        return false;
    };
    eval_return_class_type_is_a(
        &actual_resolved,
        actual_owner,
        expected_name,
        expected_owner,
        pending_class,
        context,
    ) || eval_resolve_return_class_type_name(expected_name, expected_owner, pending_class, context)
        .is_some_and(|expected_resolved| actual_resolved.eq_ignore_ascii_case(&expected_resolved))
}

/// Returns whether an actual class-like return type satisfies an expected class-like target.
fn eval_return_class_type_is_a(
    actual_name: &str,
    actual_owner: &str,
    expected_name: &str,
    expected_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    let Some(actual_resolved) =
        eval_resolve_return_class_type_name(actual_name, actual_owner, pending_class, context)
    else {
        return false;
    };
    let Some(expected_resolved) =
        eval_resolve_return_class_type_name(expected_name, expected_owner, pending_class, context)
    else {
        return false;
    };
    if actual_resolved.eq_ignore_ascii_case(&expected_resolved) {
        return true;
    }
    if pending_class.is_some_and(|class| class.name().eq_ignore_ascii_case(&actual_resolved)) {
        return pending_class_return_type_is_a(pending_class, &expected_resolved, context);
    }
    if context.has_class(&actual_resolved) {
        return context.class_is_a(&actual_resolved, &expected_resolved, false);
    }
    if context.has_interface(&actual_resolved) {
        return context
            .interface_parent_names(&actual_resolved)
            .iter()
            .any(|parent| parent.eq_ignore_ascii_case(&expected_resolved));
    }
    false
}

/// Returns whether the pending class declaration satisfies one expected class-like type.
fn pending_class_return_type_is_a(
    pending_class: Option<&EvalClass>,
    expected_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    let Some(class) = pending_class else {
        return false;
    };
    if class.name().eq_ignore_ascii_case(expected_name) {
        return true;
    }
    if class.parent().is_some_and(|parent| {
        parent.eq_ignore_ascii_case(expected_name)
            || context.class_is_a(parent, expected_name, false)
    }) {
        return true;
    }
    pending_class_return_interface_names(class, context)
        .iter()
        .any(|interface| interface.eq_ignore_ascii_case(expected_name))
}

/// Returns direct and inherited interface names for a pending eval class declaration.
fn pending_class_return_interface_names(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Vec<String> {
    let mut interfaces = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = class.parent() {
        for interface in context.class_interface_names(parent) {
            push_unique_return_class_name(&interface, &mut interfaces, &mut seen);
        }
    }
    for interface in class.interfaces() {
        push_unique_return_class_name(interface, &mut interfaces, &mut seen);
        for parent in context.interface_parent_names(interface) {
            push_unique_return_class_name(&parent, &mut interfaces, &mut seen);
        }
    }
    interfaces
}

/// Adds one class-like name once using PHP case-insensitive matching.
fn push_unique_return_class_name(
    name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    let name = name.trim_start_matches('\\');
    if seen.insert(name.to_ascii_lowercase()) {
        names.push(name.to_string());
    }
}

/// Resolves `self`/`parent`/`static` and aliases in a declared return type atom.
fn eval_resolve_return_class_type_name(
    type_name: &str,
    owner_name: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> Option<String> {
    let owner_name = owner_name.trim_start_matches('\\');
    match type_name
        .trim_start_matches('\\')
        .to_ascii_lowercase()
        .as_str()
    {
        "self" | "static" => Some(owner_name.to_string()),
        "parent" => {
            if let Some(class) = pending_class.filter(|class| {
                class
                    .name()
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(owner_name)
            }) {
                return class
                    .parent()
                    .map(|parent| parent.trim_start_matches('\\').to_string());
            }
            context
                .class(owner_name)
                .and_then(EvalClass::parent)
                .map(|parent| parent.trim_start_matches('\\').to_string())
        }
        _ => Some(
            context
                .resolve_class_like_name(type_name)
                .unwrap_or_else(|| type_name.trim_start_matches('\\').to_string()),
        ),
    }
}
