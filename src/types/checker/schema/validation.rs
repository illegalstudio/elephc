use crate::errors::CompileError;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, StmtKind, Visibility};
use crate::types::{FunctionSig, PhpType};

use super::super::Checker;

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
        ref_params,
        declared_params: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.is_some())
            .chain(method.variadic.iter().map(|_| false))
            .collect(),
        variadic: method.variadic.clone(),
    }))
}

pub(crate) fn build_constructor_param_map(methods: &[ClassMethod]) -> Vec<Option<String>> {
    let mut param_to_prop = Vec::new();
    if let Some(constructor) = methods.iter().find(|m| m.name == "__construct") {
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

pub(crate) fn visibility_rank(visibility: &Visibility) -> u8 {
    match visibility {
        Visibility::Private => 0,
        Visibility::Protected => 1,
        Visibility::Public => 2,
    }
}

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

pub(crate) fn validate_override_signature(
    checker: &Checker,
    class_name: &str,
    method: &ClassMethod,
    parent_sig: &FunctionSig,
    is_static: bool,
) -> Result<(), CompileError> {
    let kind = if is_static { "static method" } else { "method" };
    let child_sig = build_method_sig(checker, method)?;
    if method.name == "__construct" {
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
    )
}
