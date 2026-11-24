//! Purpose:
//! Converts Chopper interface, trait and enum declarations to EvalIR.
//!
//! Called from:
//! - Shared runtime statement lowering.
//!
//! Key details:
//! - Reflection retains declaration ranges and source order for enum members.

use elephc_chopper::syntax::parser::ast::{ClassMethod, ClassProperty, Stmt, StmtKind, TypeExpr, Visibility};

use crate::eval_ir::{
    EvalEnum, EvalEnumBackingType, EvalEnumCase, EvalInterface, EvalInterfaceMethod,
    EvalInterfaceProperty, EvalParameterTypeVariant, EvalStmt, EvalTrait,
};

use super::{
    Scope,
    attributes::lower_attributes,
    classes::{lower_constant, lower_members, lower_method, lower_trait_uses, lower_visibility, source_location},
    lower_expression, ordinary_class_name,
    signatures::{RelativeTypes, lower_signature, lower_type_in_scope},
};

/// Converts class-like declarations whose type parameters have not been specialized.
pub(super) fn lower_class_like(scope: &Scope<'_>, stmt: Stmt) -> Option<EvalStmt> {
    let source = source_location(stmt.span);
    let attributes = lower_attributes(scope, stmt.attributes);
    Some(match stmt.kind {
        StmtKind::InterfaceDecl { name, generics: None, extends, properties, methods, constants } => {
            if constants.iter().any(|constant| constant.visibility != Visibility::Public) {
                return None;
            }
            let name = qualify_declaration(scope, &name)?;
            let parents = extends.into_iter().map(|name| ordinary_class_name(scope, name)).collect::<Option<_>>()?;
            let properties = properties.into_iter().map(|value| lower_contract(scope, value)).collect::<Option<_>>()?;
            let methods = methods.into_iter().map(|value| lower_interface_method(scope, value)).collect::<Option<_>>()?;
            let constants = constants.into_iter().map(|value| lower_constant(scope, value)).collect::<Option<_>>()?;
            EvalStmt::InterfaceDecl(EvalInterface::with_constants_and_properties(
                name, parents, constants, properties, methods,
            ).with_source_location(source).with_attributes(attributes))
        }
        StmtKind::TraitDecl { name, generics: None, trait_uses, properties, methods, constants } => {
            let name = qualify_declaration(scope, &name)?;
            let (traits, adaptations) = lower_trait_uses(scope, trait_uses)?;
            let (properties, methods) = lower_members(scope, properties, methods)?;
            let constants = constants.into_iter().map(|value| lower_constant(scope, value)).collect::<Option<_>>()?;
            EvalStmt::TraitDecl(EvalTrait::with_constants_traits_adaptations(
                name, constants, properties, methods, traits, adaptations,
            ).with_source_location(source).with_attributes(attributes))
        }
        StmtKind::EnumDecl { name, generics: None, backing_type, cases, implements, trait_uses, methods, constants } => {
            let name = qualify_declaration(scope, &name)?;
            let backing = match backing_type {
                None => None,
                Some(TypeExpr::Int) => Some(EvalEnumBackingType::Int),
                Some(TypeExpr::Str) => Some(EvalEnumBackingType::String),
                _ => return None,
            };
            let interfaces = implements.into_iter().map(|name| ordinary_class_name(scope, name)).collect::<Option<_>>()?;
            let (traits, adaptations) = lower_trait_uses(scope, trait_uses)?;
            let mut order = cases.iter().map(|case| (case.span.col, case.name.clone()))
                .chain(constants.iter().map(|constant| (constant.span.col, constant.name.clone())))
                .collect::<Vec<_>>();
            order.sort_by_key(|(ordinal, _)| *ordinal);
            let cases = cases.into_iter().map(|case| {
                let value = match case.value {
                    Some(value) => Some(lower_expression(scope, value)?),
                    None => None,
                };
                Some(EvalEnumCase::new(case.name, value).with_attributes(lower_attributes(scope, case.attributes)))
            }).collect::<Option<_>>()?;
            let methods = methods.into_iter().map(|value| lower_method(scope, value)).collect::<Option<_>>()?;
            let constants = constants.into_iter().map(|value| lower_constant(scope, value)).collect::<Option<_>>()?;
            EvalStmt::EnumDecl(EvalEnum::with_members_traits_adaptations(
                name, backing, interfaces, cases, constants, methods, traits, adaptations,
            ).with_source_location(source).with_attributes(attributes)
                .with_member_order(order.into_iter().map(|(_, name)| name).collect()))
        }
        _ => return None,
    })
}

/// Rejects reserved class-like names before adding the declaration's namespace.
fn qualify_declaration(scope: &Scope<'_>, name: &str) -> Option<String> {
    (!crate::parser::is_reserved_word(name)).then(|| scope.qualify(name))
}

/// Retains an interface property's accessor requirements without a storage slot.
fn lower_contract(scope: &Scope<'_>, property: ClassProperty) -> Option<EvalInterfaceProperty> {
    if property.visibility != Visibility::Public || property.is_static || !property.hooks.any()
        || (property.set_visibility.is_some() && (!property.hooks.set || property.type_expr.is_none()))
    {
        return None;
    }
    let ty = lower_type_in_scope(scope, property.type_expr, RelativeTypes::Lexical)?;
    if ty.as_ref().is_some_and(|ty| ty.variants().contains(&EvalParameterTypeVariant::Callable)) {
        return None;
    }
    Some(EvalInterfaceProperty::new(property.name, property.hooks.requires_get(), property.hooks.set)
        .with_get_returns_by_ref(property.hooks.get_by_ref)
        .with_type(ty)
        .with_set_visibility(property.set_visibility.map(lower_visibility))
        .with_attributes(lower_attributes(scope, property.attributes)))
}

/// Converts a method signature that has no interface implementation body.
fn lower_interface_method(scope: &Scope<'_>, method: ClassMethod) -> Option<EvalInterfaceMethod> {
    if method.has_body || method.is_final || method.by_ref_return
        || method.visibility != Visibility::Public || !method.type_params.is_empty()
    {
        return None;
    }
    let signature = lower_signature(scope, method.params, method.variadic, method.variadic_by_ref,
        method.variadic_type, method.return_type, true)?;
    Some(EvalInterfaceMethod::new(method.name, signature.names)
        .with_static(method.is_static)
        .with_source_location(source_location(method.span))
        .with_attributes(lower_attributes(scope, method.attributes))
        .with_parameter_attributes(method.param_attributes.into_iter().map(|groups| lower_attributes(scope, groups)).collect())
        .with_parameter_types(signature.types)
        .with_parameter_defaults(signature.defaults)
        .with_parameter_by_ref_flags(signature.by_ref)
        .with_parameter_variadic_flags(signature.is_variadic)
        .with_return_type(signature.return_type))
}
