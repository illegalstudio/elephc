//! Purpose:
//! Lowers ordinary runtime class declarations from the shared Chopper AST.
//!
//! Called from:
//! - Magician's statement lowering for dynamic eval.
//!
//! Key details:
//! - The interpreter retains member visibility, defaults, signature and source metadata.
//! - Declaration kinds outside the runtime profile are refused during lowering.

use elephc_chopper::syntax::{
    parser::ast::{
        ClassConst, ClassMethod, ClassProperty, StaticReceiver, Stmt, StmtKind,
        TraitAdaptation, TraitUse, Visibility,
    },
    span::Span,
};

use crate::eval_ir::{EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalParameterTypeVariant, EvalSourceLocation, EvalStmt, EvalTraitAdaptation, EvalVisibility};

use super::{Scope, attributes::lower_attributes, lower_branch, lower_expression, ordinary_class_name, signatures::{RelativeTypes, lower_signature, lower_type_in_scope}};

/// Converts a concrete class containing ordinary properties and methods.
pub(super) fn lower_class(scope: &Scope<'_>, stmt: Stmt) -> Option<EvalClass> {
    let StmtKind::ClassDecl {
        name,
        generics: None,
        extends,
        implements,
        is_abstract,
        is_final,
        is_readonly_class,
        trait_uses,
        properties,
        methods,
        constants,
    } = stmt.kind else {
        return None;
    };
    if crate::parser::is_reserved_word(&name) {
        return None;
    }
    let parent = match extends {
        Some(name) => Some(ordinary_class_name(scope, name)?),
        None => None,
    };
    let interfaces = implements.into_iter().map(|name| ordinary_class_name(scope, name)).collect::<Option<_>>()?;
    let (traits, adaptations) = lower_trait_uses(scope, trait_uses)?;
    if is_readonly_class && properties.iter().any(|property| {
        property.hooks.any() || (!property.is_static && property.default.is_some())
    }) {
        return None;
    }
    let (properties, methods) = lower_members(scope, properties, methods)?;
    let constants = constants.into_iter().map(|constant| lower_constant(scope, constant)).collect::<Option<_>>()?;
    let anonymous = elephc_chopper::syntax::parser::is_anonymous_class_name(&name);
    let name = if anonymous { crate::parser::state::next_anonymous_class_name() } else { scope.qualify(&name) };
    let class = EvalClass::with_class_modifiers_traits_adaptations_and_constants(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
            parent,
            interfaces,
            traits,
            adaptations,
            constants,
            properties,
            methods,
        )
        .with_readonly_properties()
        .with_source_location(source_location(stmt.span))
        .with_attributes(lower_attributes(scope, stmt.attributes));
    Some(if anonymous { class.with_anonymous() } else { class })
}

/// Converts an ordinary class constant and preserves visibility and finality.
pub(super) fn lower_constant(scope: &Scope<'_>, constant: ClassConst) -> Option<EvalClassConstant> {
    if constant.type_expr.is_some() {
        return None;
    }
    Some(EvalClassConstant::with_visibility_and_final(
        constant.name,
        lower_visibility(constant.visibility),
        constant.is_final,
        lower_expression(scope, constant.value)?,
    ).with_attributes(lower_attributes(scope, constant.attributes)))
}

/// Resolves ordinary trait names and adaptation qualifiers through the active imports.
pub(super) fn lower_trait_uses(
    scope: &Scope<'_>,
    uses: Vec<TraitUse>,
) -> Option<(Vec<String>, Vec<EvalTraitAdaptation>)> {
    let mut names = Vec::new();
    let mut adaptations = Vec::new();
    for item in uses {
        names.extend(super::templates::lower_class_arguments_list(scope, item.trait_names, item.type_args)?);
        for adaptation in item.adaptations {
            adaptations.push(match adaptation {
                TraitAdaptation::Alias { trait_name, method, alias, visibility, is_final } => EvalTraitAdaptation::Alias {
                    trait_name: trait_name.map(|name| scope.class_name(&name)),
                    method,
                    alias,
                    visibility: visibility.map(lower_visibility),
                    is_final,
                },
                TraitAdaptation::InsteadOf { trait_name, method, instead_of } => EvalTraitAdaptation::InsteadOf {
                    trait_name: trait_name.map(|name| scope.class_name(&name)),
                    method,
                    instead_of: instead_of.into_iter().map(|name| scope.class_name(&name)).collect(),
                },
            });
        }
    }
    Some((names, adaptations))
}

/// Keeps relative receivers symbolic for Magician's class scope and late static binding.
pub(super) fn lower_receiver(scope: &Scope<'_>, receiver: StaticReceiver) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => ordinary_class_name(scope, name),
        StaticReceiver::Self_ => Some("self".to_owned()),
        StaticReceiver::Parent => Some("parent".to_owned()),
        StaticReceiver::Static => Some("static".to_owned()),
        StaticReceiver::Generic(ty) => Some(super::templates::lower_structural_type(scope, ty)?.to_string()),
    }
}

/// Converts ordinary property storage and its optional constant initializer.
fn lower_property(scope: &Scope<'_>, property: ClassProperty) -> Option<EvalClassProperty> {
    if (property.by_ref && !property.is_promoted)
        || (property.readonly && (property.is_static || property.default.is_some()))
    {
        return None;
    }
    if let Some(set_visibility) = &property.set_visibility {
        if property.type_expr.is_none() || property.is_static || matches!(
            (&property.visibility, set_visibility),
            (Visibility::Private, Visibility::Protected | Visibility::Public)
                | (Visibility::Protected, Visibility::Public)
        ) {
            return None;
        }
    }
    let ty = lower_type_in_scope(scope, property.type_expr, RelativeTypes::Lexical)?;
    if ty.as_ref().is_some_and(|ty| ty.variants().contains(&EvalParameterTypeVariant::Callable)) {
        return None;
    }
    let default = match property.default {
        Some(value) => Some(lower_expression(scope, value)?),
        None => None,
    };
    let value = EvalClassProperty::with_visibility_static_final_and_readonly(
        property.name,
        lower_visibility(property.visibility),
        property.is_static,
        property.is_final,
        property.readonly,
        default,
    )
    .with_type(ty)
    .with_set_visibility(property.set_visibility.map(lower_visibility))
    .with_attributes(lower_attributes(scope, property.attributes));
    Some(if property.is_abstract {
        value.with_abstract_hook_contract(property.hooks.requires_get(), property.hooks.set)
            .with_get_contract_returns_by_ref(property.hooks.get_by_ref)
    } else if property.is_promoted {
        value.with_promoted()
    } else if property.hooks.any() {
        value.with_hooks(property.hooks.requires_get(), property.hooks.set)
            .with_virtual(property.hooks.is_virtual())
    } else {
        value
    })
}

/// Lowers properties and methods together so hooks retain their source signatures.
pub(super) fn lower_members(
    scope: &Scope<'_>,
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
) -> Option<(Vec<EvalClassProperty>, Vec<EvalClassMethod>)> {
    use elephc_chopper::syntax::names::{property_hook_get_method, property_hook_set_method};

    let mut lowered = properties.iter().cloned()
        .map(|property| lower_property(scope, property))
        .collect::<Option<Vec<_>>>()?;
    for (property, metadata) in properties.iter().zip(&mut lowered) {
        for (present, hook_name) in [
            (property.hooks.requires_get(), property_hook_get_method(&property.name)),
            (property.hooks.set, property_hook_set_method(&property.name)),
        ] {
            if !present {
                continue;
            }
            let count = methods.iter().filter(|method| method.name == hook_name).count();
            if count != usize::from(!property.is_abstract) {
                return None;
            }
        }
        if property.hooks.set && !property.is_abstract {
            let setter = methods.iter().find(|method| method.name == property_hook_set_method(&property.name))?;
            let (_, ty, ..) = setter.params.first()?;
            if ty.is_some() && property.type_expr.is_none() {
                return None;
            }
            *metadata = metadata.clone().with_set_hook_type(lower_type_in_scope(
                scope, ty.clone(), RelativeTypes::Lexical,
            )?);
        }
    }
    let methods = methods.into_iter().map(|mut method| {
        if let Some(property) = properties.iter().find(|property| {
            !property.is_abstract && property.hooks.matches_accessor(&property.name, &method.name)
        }) {
            // The hook invokes under the property's contract. Its generated native method
            // signature supplies other types that Magician must not enforce as a user method.
            method.return_type = None;
            if let Some((_, ty, ..)) = method.params.first_mut() {
                if ty.is_none() {
                    *ty = property.type_expr.clone();
                }
            }
        }
        lower_method(scope, method)
    }).collect::<Option<_>>()?;
    Some((lowered, methods))
}

/// Converts an ordinary method body, return type and parameters in source order.
pub(super) fn lower_method(scope: &Scope<'_>, mut method: ClassMethod) -> Option<EvalClassMethod> {
    if method.has_body == method.is_abstract {
        return None;
    }
    let parameters = std::mem::take(&mut method.type_params);
    if parameters.is_empty() {
        return lower_method_body(scope, method);
    }
    let type_scope = scope.with_type_parameters(&parameters);
    let span = method.span;
    let lowered = lower_method_body(&type_scope, method)?;
    let prototype = EvalStmt::FunctionDecl {
        name: lowered.name().to_owned(),
        source_location: lowered.source_location(),
        attributes: Vec::new(),
        params: lowered.params().to_vec(),
        parameter_attributes: lowered.parameter_attributes().to_vec(),
        parameter_types: lowered.parameter_types().to_vec(),
        parameter_defaults: lowered.parameter_defaults().to_vec(),
        parameter_is_by_ref: lowered.parameter_is_by_ref().to_vec(),
        parameter_is_variadic: lowered.parameter_is_variadic().to_vec(),
        return_type: lowered.return_type().cloned(),
        body: lowered.body().to_vec(),
    };
    let template = super::templates::retain_template(
        &type_scope, parameters, lowered.name().to_owned(), span, prototype,
    )?;
    Some(lowered.with_generic_template(template))
}

/// Lowers a method signature and executable body without constructing its template metadata.
fn lower_method_body(scope: &Scope<'_>, method: ClassMethod) -> Option<EvalClassMethod> {
    let signature = lower_signature(
        scope,
        method.params,
        method.variadic,
        method.variadic_by_ref,
        method.variadic_type,
        method.return_type,
        true,
    )?;
    let body = lower_branch(scope, method.body)?;
    super::validate_body_jumps(scope, &body)?;
    Some(EvalClassMethod::with_visibility_and_modifiers(
        method.name,
        lower_visibility(method.visibility),
        method.is_static,
        method.is_abstract,
        method.is_final,
        signature.names,
        body,
    )
    .with_source_location(source_location(method.span))
    .with_attributes(lower_attributes(scope, method.attributes))
    .with_parameter_attributes(method.param_attributes.into_iter().map(|groups| lower_attributes(scope, groups)).collect())
    .with_parameter_types(signature.types)
    .with_parameter_defaults(signature.defaults)
    .with_parameter_by_ref_flags(signature.by_ref)
    .with_parameter_variadic_flags(signature.is_variadic)
    .with_returns_by_ref(method.by_ref_return)
    .with_return_type(signature.return_type)
    .with_strict_types(scope.strict_types))
}

/// Maps declaration visibility into the interpreter's runtime metadata.
pub(super) fn lower_visibility(visibility: Visibility) -> EvalVisibility {
    match visibility {
        Visibility::Public => EvalVisibility::Public,
        Visibility::Protected => EvalVisibility::Protected,
        Visibility::Private => EvalVisibility::Private,
    }
}

/// Converts one retained declaration span to the interpreter's line range.
pub(super) fn source_location(span: Span) -> EvalSourceLocation {
    EvalSourceLocation::new(i64::from(span.line), i64::from(span.end_line))
}
