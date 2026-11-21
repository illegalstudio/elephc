//! Purpose:
//! Converts Chopper declaration signatures to Magician's EvalIR metadata.
//!
//! Called from:
//! - Lowering of dynamic functions, methods, and closures.
//!
//! Key details:
//! - Parameter types, defaults, and reference flags follow written order.

use elephc_chopper::syntax::{names::NameKind, parser::ast::{AttributeGroup, Expr, Stmt, TypeExpr}, span::Span};

use crate::eval_ir::{EvalExpr, EvalParameterType, EvalParameterTypeVariant, EvalSourceLocation, EvalStmt};

use super::{attributes::lower_attributes, Scope, lower_branch, lower_expression, ordinary_class_name};

/// Converts a function signature and retains the line range used by reflection.
pub(super) fn lower_function(
    scope: &Scope<'_>,
    name: String,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    attributes: Vec<AttributeGroup>,
    param_attributes: Vec<Vec<AttributeGroup>>,
    variadic: Option<String>,
    variadic_by_ref: bool,
    variadic_type: Option<TypeExpr>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
    span: Span,
) -> Option<EvalStmt> {
    let signature = lower_signature(scope, params, variadic, variadic_by_ref, variadic_type, return_type, false)?;
    let body = lower_branch(scope, body)?;
    super::validate_body_jumps(scope, &body)?;
    Some(EvalStmt::FunctionDecl {
        name: scope.qualify(&name),
        source_location: Some(EvalSourceLocation::new(i64::from(span.line), i64::from(span.end_line))),
        attributes: lower_attributes(scope, attributes),
        params: signature.names,
        parameter_attributes: param_attributes.into_iter().map(|groups| lower_attributes(scope, groups)).collect(),
        parameter_types: signature.types,
        parameter_defaults: signature.defaults,
        parameter_is_by_ref: signature.by_ref,
        parameter_is_variadic: signature.is_variadic,
        return_type: signature.return_type,
        body,
    })
}

/// Source-order signature data shared by function and method lowering.
pub(super) struct LoweredSignature {
    pub(super) names: Vec<String>,
    pub(super) types: Vec<Option<EvalParameterType>>,
    pub(super) defaults: Vec<Option<EvalExpr>>,
    pub(super) by_ref: Vec<bool>,
    pub(super) is_variadic: Vec<bool>,
    pub(super) return_type: Option<EvalParameterType>,
}

/// Converts parameters and their defaults using the interpreter's type metadata.
pub(super) fn lower_signature(
    scope: &Scope<'_>,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    variadic: Option<String>,
    variadic_by_ref: bool,
    variadic_type: Option<TypeExpr>,
    return_type: Option<TypeExpr>,
    is_method: bool,
) -> Option<LoweredSignature> {
    let param_scope = if is_method { RelativeTypes::Lexical } else { RelativeTypes::None };
    let count = params.len() + usize::from(variadic.is_some());
    let mut names = Vec::with_capacity(count);
    let mut types = Vec::with_capacity(count);
    let mut defaults = Vec::with_capacity(count);
    let mut by_ref = Vec::with_capacity(count);
    for (name, ty, default, parameter_by_ref) in params {
        names.push(name);
        types.push(lower_type_in_scope(scope, ty, param_scope)?);
        defaults.push(lower_parameter_default(scope, default)?);
        by_ref.push(parameter_by_ref);
    }
    let mut is_variadic = vec![false; names.len()];
    if let Some(variadic) = variadic {
        names.push(variadic);
        types.push(lower_type_in_scope(scope, variadic_type, param_scope)?);
        defaults.push(None);
        by_ref.push(variadic_by_ref);
        is_variadic.push(true);
    }
    Some(LoweredSignature {
        names, types, defaults, by_ref, is_variadic,
        return_type: match return_type {
            Some(TypeExpr::Void) => Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Void], false)),
            Some(TypeExpr::Never) => Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Never], false)),
            ty => lower_type_in_scope(
                scope,
                ty,
                if is_method { RelativeTypes::LateStatic } else { RelativeTypes::None },
            )?,
        },
    })
}

/// Converts a parameter default only when Magician supports materializing it.
fn lower_parameter_default(scope: &Scope<'_>, value: Option<Expr>) -> Option<Option<EvalExpr>> {
    let Some(value) = value else { return Some(None) };
    let value = lower_expression(scope, value)?;
    crate::parser::method_parameter_default_is_supported(&value).then_some(Some(value))
}

/// Converts a PHP type hint to eval signature metadata in source order.
pub(super) fn lower_optional_type(scope: &Scope<'_>, ty: Option<TypeExpr>) -> Option<Option<EvalParameterType>> {
    lower_type_in_scope(scope, ty, RelativeTypes::None)
}

/// Which class-relative type spellings are available at the declaration site.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RelativeTypes {
    None,
    Lexical,
    LateStatic,
}

/// Converts a hint while preserving class-relative names in member signatures.
pub(super) fn lower_type_in_scope(
    scope: &Scope<'_>,
    ty: Option<TypeExpr>,
    relative: RelativeTypes,
) -> Option<Option<EvalParameterType>> {
    let Some(ty) = ty else { return Some(None) };
    let ty = match ty {
        TypeExpr::Nullable(atom) => {
            let variants = lower_type_variant(scope, *atom, relative)?.into_iter().collect();
            EvalParameterType::new(variants, true)
        }
        TypeExpr::Union(members) => {
            let mut nullable = false;
            let mut variants = Vec::with_capacity(members.len());
            for member in members {
                match lower_type_variant(scope, member, relative)? {
                    Some(variant) => variants.push(variant),
                    None => nullable = true,
                }
            }
            EvalParameterType::new(variants, nullable)
        }
        TypeExpr::Intersection(members) => {
            let variants = members.into_iter()
                .map(|member| lower_type_variant(scope, member, relative).flatten())
                .collect::<Option<_>>()?;
            EvalParameterType::intersection(variants)
        }
        atom => match lower_type_variant(scope, atom, relative)? {
            Some(variant) => EvalParameterType::new(vec![variant], false),
            None => EvalParameterType::new(Vec::new(), true),
        },
    };
    Some(Some(ty))
}

/// Maps one type atom, retaining explicit null separately from non-null variants.
fn lower_type_variant(
    scope: &Scope<'_>,
    atom: TypeExpr,
    relative: RelativeTypes,
) -> Option<Option<EvalParameterTypeVariant>> {
    let variant = match atom {
        TypeExpr::Int => EvalParameterTypeVariant::Int,
        TypeExpr::Float => EvalParameterTypeVariant::Float,
        TypeExpr::Bool => EvalParameterTypeVariant::Bool,
        TypeExpr::Str => EvalParameterTypeVariant::String,
        TypeExpr::Iterable => EvalParameterTypeVariant::Iterable,
        TypeExpr::False => EvalParameterTypeVariant::Generic(elephc_generics::Type::atom("false")),
        ty @ (TypeExpr::Array(_) | TypeExpr::AssocArray { .. } | TypeExpr::GenericClass { .. } | TypeExpr::CallableSig { .. }) => {
            EvalParameterTypeVariant::Generic(super::templates::lower_structural_type(scope, ty)?)
        }
        TypeExpr::Named(name) if name.kind == NameKind::Unqualified => {
            match name.as_str().to_ascii_lowercase().as_str() {
                "array" => EvalParameterTypeVariant::Array,
                "callable" => EvalParameterTypeVariant::Callable,
                "object" => EvalParameterTypeVariant::Object,
                "mixed" => EvalParameterTypeVariant::Mixed,
                "null" => return Some(None),
                "true" => EvalParameterTypeVariant::Class(scope.class_name(&name)),
                name @ ("self" | "parent") if relative != RelativeTypes::None => {
                    EvalParameterTypeVariant::Class(name.to_owned())
                }
                "static" if relative == RelativeTypes::LateStatic => {
                    EvalParameterTypeVariant::Class("static".to_owned())
                }
                _ => EvalParameterTypeVariant::Class(ordinary_class_name(scope, name)?),
            }
        }
        TypeExpr::Named(name) => EvalParameterTypeVariant::Class(scope.class_name(&name)),
        _ => return None,
    };
    Some(Some(variant))
}
