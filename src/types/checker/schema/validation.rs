//! Purpose:
//! Validates schema validation declarations for the checker.
//! Turns parsed declarations into canonical metadata and rejects invalid contracts before code generation.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Declaration metadata must align with name resolution, inheritance flattening, and runtime/codegen expectations.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Attribute, ClassMethod, Expr, ExprKind, StmtKind, Visibility};
use crate::types::{FunctionSig, PhpType};

use super::super::Checker;

/// Builds a `FunctionSig` from a parsed class method, resolving parameter and return type
/// annotations through the checker. Parameters without type hints default to `PhpType::Int`.
/// Validates that each declared parameter's default value is compatible with its resolved type.
/// Infers return type from method body when no return annotation is present.
pub(crate) fn build_method_sig(
    checker: &Checker,
    method: &ClassMethod,
) -> Result<FunctionSig, CompileError> {
    let params: Vec<(String, PhpType)> = method
        .params
        .iter()
        .map(|(n, type_ann, _, _)| {
            let ty = match type_ann {
                Some(type_ann) => checker.resolve_declared_param_type_hint(
                    type_ann,
                    method.span,
                    &format!("Method parameter ${}", n),
                )?,
                None => PhpType::Int,
            };
            Ok((n.clone(), ty))
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    let defaults: Vec<Option<Expr>> = method.params.iter().map(|(_, _, d, _)| d.clone()).collect();
    let ref_params: Vec<bool> = method.params.iter().map(|(_, _, _, r)| *r).collect();
    for ((param_name, type_ann, default, _), (_, resolved_ty)) in
        method.params.iter().zip(params.iter())
    {
        if type_ann.is_some() {
            checker.validate_declared_default_type(
                resolved_ty,
                default.as_ref(),
                method.span,
                &format!("Method parameter ${}", param_name),
            )?;
        }
    }
    let return_type = match method.return_type.as_ref() {
        Some(type_ann) => checker.resolve_declared_return_type_hint(
            type_ann,
            method.span,
            &format!("Method '{}'", method.name),
        )?,
        None => super::super::infer_return_type_syntactic(&method.body),
    };
    Ok(Checker::callable_wrapper_sig(&FunctionSig {
        params,
        defaults,
        return_type,
        declared_return: method.return_type.is_some(),
        ref_params,
        declared_params: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.is_some())
            .chain(method.variadic.iter().map(|_| false))
            .collect(),
        variadic: method.variadic.clone(),
        deprecation: extract_deprecation(&method.attributes),
    }))
}

/// Returns `Some(reason)` when the attribute list contains a `#[\Deprecated]`
/// marker, with `reason` set to the attribute's first string argument (or an
/// empty string if absent). Match is case-insensitive on the last segment of
/// the attribute name.
pub(crate) fn extract_deprecation(
    groups: &[crate::parser::ast::AttributeGroup],
) -> Option<String> {
    for group in groups {
        for attr in &group.attributes {
            if !matches_global_builtin_attribute(attr, "Deprecated") {
                continue;
            }
            let reason = attr.args.iter().find_map(|expr| match &expr.kind {
                ExprKind::StringLiteral(s) => Some(s.clone()),
                _ => None,
            });
            return Some(reason.unwrap_or_default());
        }
    }
    None
}

/// Returns `true` if `attr` is a global builtin attribute matching `builtin` by name.
/// Fully-qualified names must match exactly (case-insensitive); unqualified names
/// match the last segment case-insensitively. Used to detect `#[\Deprecated]` and similar.
pub(crate) fn matches_global_builtin_attribute(attr: &Attribute, builtin: &str) -> bool {
    let name = attr.name.as_canonical();
    if attr.name.is_fully_qualified() {
        return name.eq_ignore_ascii_case(builtin);
    }
    attr.name.is_unqualified() && name.eq_ignore_ascii_case(builtin)
}

/// Builds a mapping from constructor parameter index to property name for each parameter.
/// For each parameter, searches constructor body for `PropertyAssign` statements where
/// the right-hand side is a Variable with the same name as the parameter; if found,
/// returns `Some(property_name)`, otherwise `None`. Returns empty vec if no constructor.
pub(crate) fn build_constructor_param_map(methods: &[ClassMethod]) -> Vec<Option<String>> {
    let mut param_to_prop = Vec::new();
    if let Some(constructor) = methods
        .iter()
        .find(|m| php_symbol_key(&m.name) == "__construct")
    {
        param_to_prop = constructor
            .params
            .iter()
            .map(|(pname, _, _, _)| {
                for stmt in &constructor.body {
                    if let StmtKind::PropertyAssign {
                        property, value, ..
                    } = &stmt.kind
                    {
                        if let ExprKind::Variable(vn) = &value.kind {
                            if vn == pname {
                                return Some(property.clone());
                            }
                        }
                    }
                }
                None
            })
            .collect();
    }
    param_to_prop
}

/// Returns a numeric rank for visibility levels: `private=0`, `protected=1`, `public=2`.
/// Used to enforce that overriding methods are not less visible than the parent method.
pub(crate) fn visibility_rank(visibility: &Visibility) -> u8 {
    match visibility {
        Visibility::Private => 0,
        Visibility::Protected => 1,
        Visibility::Public => 2,
    }
}

/// Counts how many parameters in `sig` are required (have no default).
/// The variadic parameter, if present, is never considered required even if it has no default.
pub(crate) fn required_param_count(sig: &FunctionSig) -> usize {
    sig.defaults
        .iter()
        .enumerate()
        .filter(|(idx, default)| {
            if sig.variadic.is_some() && *idx + 1 == sig.defaults.len() {
                return false;
            }
            default.is_none()
        })
        .count()
}

/// Validates that `child_sig` is compatible with `parent_sig` for override purposes.
/// Checks parameter count, ref params, defaults layout, variadic flag, and required param count.
/// Reports errors with `context` and `kind` (e.g., "overriding method") in the message.
pub(crate) fn validate_signature_compatibility(
    span: crate::span::Span,
    owner_name: &str,
    method_name: &str,
    child_sig: &FunctionSig,
    parent_sig: &FunctionSig,
    kind: &str,
    context: &str,
) -> Result<(), CompileError> {
    if child_sig.params.len() != parent_sig.params.len() {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child_sig.ref_params != parent_sig.ref_params {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change pass-by-reference parameters when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    let child_defaults: Vec<bool> = child_sig
        .defaults
        .iter()
        .map(|default| default.is_some())
        .collect();
    let parent_defaults: Vec<bool> = parent_sig
        .defaults
        .iter()
        .map(|default| default.is_some())
        .collect();
    if child_defaults != parent_defaults {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change optional parameter layout when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child_sig.variadic != parent_sig.variadic {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change variadic parameter shape when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if required_param_count(child_sig) != required_param_count(parent_sig) {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change required parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    Ok(())
}

/// Returns `true` if `actual` is a compatible declared return type for `expected`.
/// Allows `PhpType::Never` (unreachable) as a wildcard match. Otherwise delegates to
/// `checker.type_accepts(expected, actual)` for standard subtype checking.
pub(crate) fn declared_return_type_compatible(
    checker: &Checker,
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    matches!(actual, PhpType::Never) || checker.type_accepts(expected, actual)
}

/// Validates that `method` can override `parent_sig` in class `class_name`.
/// Builds the child signature via `build_method_sig`, skips validation for `__construct`,
/// checks signature compatibility, and ensures the child does not remove a declared
/// return type when the parent has one or make it incompatible.
pub(crate) fn validate_override_signature(
    checker: &Checker,
    class_name: &str,
    method: &ClassMethod,
    parent_sig: &FunctionSig,
    is_static: bool,
) -> Result<(), CompileError> {
    let kind = if is_static { "static method" } else { "method" };
    let child_sig = build_method_sig(checker, method)?;
    if php_symbol_key(&method.name) == "__construct" {
        return Ok(());
    }
    validate_signature_compatibility(
        method.span,
        class_name,
        &method.name,
        &child_sig,
        parent_sig,
        kind,
        "overriding",
    )?;
    if parent_sig.declared_return && !child_sig.declared_return {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} without declaring a compatible return type (parent returns {})",
                kind, class_name, method.name, parent_sig.return_type
            ),
        ));
    }
    if parent_sig.declared_return
        && !declared_return_type_compatible(checker, &parent_sig.return_type, &child_sig.return_type)
    {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} with incompatible return type {} (parent returns {})",
                kind, class_name, method.name, child_sig.return_type, parent_sig.return_type
            ),
        ));
    }
    Ok(())
}
