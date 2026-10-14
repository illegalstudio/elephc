//! Purpose:
//! Specializes eval generic declarations using runtime argument types and shared AOT binding rules.
//!
//! Called from:
//! - Function dispatch, object construction, static calls, and class relation registration.
//!
//! Key details:
//! - Arguments arrive already evaluated; specialization borrows them and never evaluates source again.
//! - Concrete declarations use ordinary EvalIR execution, ownership, and PHP call binding.

use super::*;
use crate::eval_ir::{generic_type, EvalGenericTemplate};
use elephc_generics::{Bindings, Type};

mod locals;
pub(in crate::interpreter) use locals::*;
mod types;
mod variance;
pub(in crate::interpreter) use variance::*;
pub(in crate::interpreter) use types::*;

/// Decodes a written specialization name using Magician's type grammar.
pub(super) fn split_generic_name(name: &str) -> Result<(String, Option<Vec<Type>>), EvalStatus> {
    if !name.contains('<') { return Ok((name.to_string(), None)); }
    match crate::parser::generics::parse_type_string(name).map_err(|_| EvalStatus::ParseError)? {
        Type::Apply(base, args) => Ok((base, Some(args))),
        _ => Err(EvalStatus::ParseError),
    }
}

/// Formats a shared binding failure as a catchable runtime TypeError.
fn binding_error<T>(name: &str, error: elephc_generics::Error, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<T, EvalStatus> {
    let detail = match error {
        elephc_generics::Error::Unconstrained(param) => format!("does not determine type parameter <{}>", param.trim_start_matches("@ctor_")),
        elephc_generics::Error::Conflict { param, first, second } => format!("binds type parameter <{param}> to both {first} and {second}"),
        elephc_generics::Error::TooDeep { param, ty } => format!("instantiates an ever-deeper type: <{param}> reached {ty}"),
        elephc_generics::Error::TooMany { written, declared } => format!("writes {written} type arguments, but declares {declared}"),
        elephc_generics::Error::Unbound { param, written } => format!("writes {written} type arguments but leaves <{param}> unbound, and it declares no default"),
    };
    eval_throw_type_error(&format!("Generic '{name}' {detail}"), context, values)
}

/// Returns inference metadata from a function or a selected class method.
fn template_signature(template: &EvalGenericTemplate, method: &str) -> (Vec<String>, Vec<Option<EvalParameterType>>, Vec<bool>) {
    match template.prototype.as_ref() {
        EvalStmt::FunctionDecl { params, parameter_types, parameter_is_variadic, .. } => (params.clone(), parameter_types.clone(), parameter_is_variadic.clone()),
        EvalStmt::ClassDecl(class) => class.method(method).map(|method| (method.params().to_vec(), method.parameter_types().to_vec(), method.parameter_is_variadic().to_vec())).unwrap_or_default(),
        _ => (Vec::new(), Vec::new(), Vec::new()),
    }
}

/// Binds generic slots from declaration-ordered arguments or a fully written argument list.
fn bind_template(
    template: &EvalGenericTemplate, written: Option<&[Type]>, args: &[EvaluatedCallArg], method: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Bindings, EvalStatus> {
    let parameters = template.parameters.iter().chain(&template.constructor_parameters).cloned().collect::<Vec<_>>();
    // Class-only spelling leaves constructor slots inferable; longer lists bind both sets explicitly.
    let written_parameters = if template.parameters.is_empty()
        || written.is_some_and(|arguments| arguments.len() > template.parameters.len()) {
        &parameters
    } else { &template.parameters };
    let mut inferred = Vec::new();
    if let Some(written) = written {
        inferred = match elephc_generics::written(written_parameters, written) {
            Ok(bindings) => bindings,
            Err(error) => return binding_error(&template.name, error, context, values),
        };
    }
    let names = parameters.iter().filter(|param| written.is_none() || !written_parameters.contains(param))
        .map(|param| param.name.clone()).collect::<Vec<_>>();
    let (params, types, variadic) = template_signature(template, method);
    let aliases = match template.prototype.as_ref() {
        EvalStmt::ClassDecl(class) if method.eq_ignore_ascii_case("__construct") => class.method("__construct").and_then(|method| method.generic_template())
            .map(|constructor| constructor.parameters.iter().map(|param| (param.name.clone(), Type::atom(format!("@ctor_{}", param.name)))).collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    let variadic_index = variadic.iter().position(|flag| *flag);
    let mut position = 0;
    for arg in args {
        let index = if let Some(name) = &arg.name { params.iter().position(|param| param == name).or(variadic_index) }
            else { let index = Some(variadic_index.map_or(position, |tail| position.min(tail))); position += 1; index };
        let Some(declared) = index.and_then(|index| types.get(index)).and_then(Option::as_ref) else { continue; };
        let declared = generic_type(declared).substitute(&aliases);
        let actual = generic_inference_type(&runtime_generic_type(arg.value, context, values, 0)?, context);
        if matches!(&declared, Type::Nullable(_)) && actual == Type::atom("null") { continue; }
        if matches!(template.prototype.as_ref(), EvalStmt::ClassDecl(_))
            && (actual == Type::atom("null") || matches!(values.type_tag(arg.value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) && values.array_len(arg.value)? == 0) { continue; }
        let unified = if let (Type::Callable(params, ret), Type::Callable(actual_params, actual_ret)) = (&declared, &actual) {
            let mut result = Ok(());
            for (declared, actual) in params.iter().zip(actual_params).chain(std::iter::once((ret.as_ref(), actual_ret.as_ref()))) {
                result = result.and_then(|()| elephc_generics::unify(&names, declared, actual, &mut inferred));
            }
            result
        } else { elephc_generics::unify(&names, &declared, &actual, &mut inferred) };
        if let Err(error) = unified { return binding_error(&template.name, error, context, values); }
    }
    let bindings = match elephc_generics::finish(&parameters, &inferred) {
        Ok(bindings) => bindings,
        Err(error) => return binding_error(&template.name, error, context, values),
    };
    for (param, (_, actual)) in parameters.iter().zip(&bindings) {
        let Some(bound) = &param.bound else { continue; };
        let bound = bound.substitute(&bindings);
        if !generic_type_is_a(actual, &bound, context, values)? {
            return eval_throw_type_error(&format!("Generic '{}' binds <{}> to {}, which does not satisfy its bound {}", template.name, param.name, actual, bound), context, values);
        }
    }
    Ok(bindings)
}

/// Selects the concrete ordinary name for one set of declaration-order bindings.
fn specialization_name(template: &EvalGenericTemplate, bindings: &Bindings) -> String {
    if bindings.is_empty() { template.name.clone() }
    else { Type::Apply(template.name.clone(), bindings.iter().map(|(_, ty)| ty.clone()).collect()).to_string() }
}

/// Dispatches a generic function while preserving the existing evaluated argument owner boundary.
pub(super) fn eval_generic_function(
    template: &EvalGenericTemplate, written: Option<&[Type]>, args: &[EvalCallArg], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |arguments, context, _, values| {
        let function = specialize_generic_function(template, written, &arguments, context, values)?;
        eval_dynamic_function_with_evaluated_args(&function, arguments, context, values)
    })
}

/// Creates an ordinary callable once, without requiring a live caller scope during registration.
fn specialize_generic_function(template: &EvalGenericTemplate, written: Option<&[Type]>, arguments: &[EvaluatedCallArg], context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<EvalFunction, EvalStatus> {
    let bindings = bind_template(template, written, arguments, "", context, values)?;
    let name = specialization_name(template, &bindings);
    if context.function(&name.to_ascii_lowercase()).is_none() {
        let declaration = crate::parser::generics::instantiate(template, &bindings, &name).map_err(|_| EvalStatus::ParseError)?;
        execute_stmt(&declaration, context, &mut ElephcEvalScope::new(), values)?;
    }
    context.function(&name.to_ascii_lowercase()).cloned().ok_or(EvalStatus::RuntimeFatal)
}

/// Resolves a template for evaluated callback or generated-code calls before ordinary binding.
pub(in crate::interpreter) fn generic_function_for_call(name: &str, arguments: &[EvaluatedCallArg], context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<Option<EvalFunction>, EvalStatus> {
    let (base, written) = split_generic_name(name)?;
    let Some(template) = context.generic_function_template(&base).cloned() else { return Ok(None); };
    specialize_generic_function(&template, written.as_deref(), arguments, context, values).map(Some)
}

/// Recognizes callable template names without exposing unspecialized PHP declarations.
pub(in crate::interpreter) fn generic_function_is_callable(name: &str, context: &ElephcEvalContext) -> bool {
    split_generic_name(name).is_ok_and(|(base, _)| context.generic_function_template(&base).is_some())
}

/// Ensures a class-like specialization exists before construction, relation checks, or static use.
/// Rechecks variance against current dependencies before returning even a cached specialization.
pub(super) fn ensure_generic_class(
    name: &str, args: Option<&[EvaluatedCallArg]>, method: &str, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let resolved = context.resolve_class_like_name(name);
    let (base, written) = match split_generic_name(resolved.as_deref().unwrap_or(name)) {
        Ok(split) => split,
        Err(_) if resolved.is_some() => return Ok(name.to_string()),
        Err(error) => return Err(error),
    };
    let Some(template) = context.generic_template(&base).cloned() else { return Ok(name.to_string()); };
    if matches!(template.prototype.as_ref(), EvalStmt::FunctionDecl { .. }) { return Err(EvalStatus::RuntimeFatal); }
    verify_generic_variance(&template, context, values)?;
    if resolved.is_some() { return Ok(name.to_string()); }
    let mut bare = None;
    if written.is_none() && args.is_none() {
        let mut bindings = Vec::new();
        for param in if template.parameters.is_empty() { &template.constructor_parameters } else { &template.parameters } {
            let ty = param.bound.as_ref().or(param.default.as_ref()).cloned().unwrap_or_else(|| Type::atom("mixed")).substitute(&bindings);
            bindings.push((param.name.clone(), ty));
        }
        bare = Some(bindings.into_iter().map(|(_, ty)| ty).collect::<Vec<_>>());
    }
    let mut binding_template = template.clone();
    if args.is_none() {
        for param in &mut binding_template.constructor_parameters {
            param.default = Some(param.bound.as_ref().or(param.default.as_ref()).cloned().unwrap_or_else(|| Type::atom("mixed")));
        }
    }
    let bindings = bind_template(&binding_template, written.as_deref().or(bare.as_deref()), args.unwrap_or(&[]), method, context, values)?;
    let concrete = specialization_name(&template, &bindings);
    if context.resolve_class_like_name(&concrete).is_some() { return Ok(concrete); }
    if !context.begin_generic_specialization(&concrete) {
        return eval_throw_type_error(&format!("Recursive generic inheritance at '{concrete}'"), context, values);
    }
    let result = crate::parser::generics::instantiate(&template, &bindings, &concrete)
        .map_err(|_| EvalStatus::ParseError)
        .and_then(|declaration| execute_stmt(&declaration, context, scope, values).map(|_| ()));
    context.end_generic_specialization(&concrete);
    result.map(|()| concrete)
}

/// Reads a bare class template as its bound/default/mixed instantiation without executing it.
pub(in crate::interpreter) fn erased_generic_type(template: &EvalGenericTemplate) -> Type {
    let mut bindings = Vec::new();
    for param in template.parameters.iter().chain(&template.constructor_parameters) {
        let ty = param.bound.as_ref().or(param.default.as_ref()).cloned().unwrap_or_else(|| Type::atom("mixed")).substitute(&bindings);
        bindings.push((param.name.clone(), ty));
    }
    crate::parser::generics::parse_type_string(&specialization_name(template, &bindings)).unwrap_or_else(|_| Type::atom(template.name.clone()))
}

/// Checks a concrete type against a substituted bound through dynamic and native class metadata.
pub(in crate::interpreter) fn generic_type_is_a(actual: &Type, bound: &Type, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    if generic_signature_accepts(actual, bound, context) || *bound == Type::atom("mixed") { return Ok(true); }
    if let (Type::Apply(actual_name, actual_args), Type::Apply(bound_name, bound_args)) = (actual, bound) {
        let actual_args = generic_class_arguments(actual_name, actual_args, context);
        let bound_args = generic_class_arguments(bound_name, bound_args, context);
        if actual_name.eq_ignore_ascii_case(bound_name) && actual_args.len() == bound_args.len() {
            if let Some(template) = context.generic_template(actual_name).filter(|template| template.parameters.len() == actual_args.len()) {
                for (param, (actual, expected)) in template.parameters.iter().zip(actual_args.iter().zip(bound_args)) {
                    if actual == expected { continue; }
                    if !variance::object_type(actual) || !variance::object_type(expected) { return Ok(false); }
                    let accepted = match param.variance {
                        1 => generic_type_is_a(actual, expected, context, values)?,
                        -1 => generic_type_is_a(expected, actual, context, values)?,
                        _ => false,
                    };
                    if !accepted { return Ok(false); }
                }
                return Ok(true);
            }
        }
    }
    if let Type::Nullable(inner) = bound {
        if *actual == Type::atom("null") { return Ok(true); }
        return generic_type_is_a(if let Type::Nullable(inner) = actual { inner } else { actual }, inner, context, values);
    }
    if let Type::Union(types) = actual {
        for ty in types { if !generic_type_is_a(ty, bound, context, values)? { return Ok(false); } }
        return Ok(true);
    }
    if let Type::Intersection(types) = bound {
        for ty in types { if !generic_type_is_a(actual, ty, context, values)? { return Ok(false); } }
        return Ok(true);
    }
    if let Type::Union(types) = bound {
        for ty in types { if generic_type_is_a(actual, ty, context, values)? { return Ok(true); } }
        return Ok(false);
    }
    let actual = actual.to_string();
    let bound = bound.to_string();
    if context.class_is_a(&actual, &bound, false) { return Ok(true); }
    let name = values.string(&actual)?;
    let result = values.object_is_a(name, &bound, false);
    let released = values.release(name);
    result.and_then(|value| released.map(|()| value))
}

/// Specializes a method at its own slots after the receiver's class arguments were substituted.
pub(super) fn specialize_generic_method(class_name: &str, method: &EvalClassMethod, arguments: &[EvaluatedCallArg], context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<EvalClassMethod, EvalStatus> {
    let Some(template) = method.generic_template() else { return Ok(method.clone()); };
    let (_, written) = split_generic_name(method.name())?;
    let bindings = bind_template(template, written.as_deref(), arguments, "", context, values)?;
    let name = specialization_name(template, &bindings);
    let key = format!("{class_name}::{name}").to_ascii_lowercase();
    if let Some(method) = context.generic_method(&key) { return Ok(method.clone()); }
    let specialized = crate::parser::generics::instantiate_method(template, &bindings, &name, method)
        .map(|specialized| specialized.with_attributes(method.attributes().to_vec()).with_original_trait_origin(method))
        .map_err(|_| EvalStatus::ParseError)?;
    context.cache_generic_method(key, specialized.clone());
    Ok(specialized)
}

/// Resolves bare adaptation names against uniquely selected concrete trait uses.
pub(in crate::interpreter) fn concrete_trait_adaptations(adaptations: &[EvalTraitAdaptation], traits: &[String]) -> Vec<EvalTraitAdaptation> {
    let resolve = |name: &mut String| {
        if traits.iter().any(|used| used.eq_ignore_ascii_case(name)) { return; }
        let matches = traits.iter().filter(|used| used.split('<').next().is_some_and(|base| base.eq_ignore_ascii_case(name))).collect::<Vec<_>>();
        if matches.len() == 1 { *name = matches[0].clone(); }
    };
    adaptations.iter().cloned().map(|mut adaptation| {
        match &mut adaptation {
            EvalTraitAdaptation::Alias { trait_name, .. } => { if let Some(name) = trait_name { resolve(name); } }
            EvalTraitAdaptation::InsteadOf { trait_name, instead_of, .. } => {
                if let Some(name) = trait_name { resolve(name); }
                for name in instead_of { resolve(name); }
            }
        }
        adaptation
    }).collect()
}

/// Instantiates statically named class storage before the corresponding write or unset.
pub(in crate::interpreter) fn specialize_static_storage_statement(stmt: &EvalStmt, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps) -> Result<Option<EvalStmt>, EvalStatus> {
    let name = match stmt {
        EvalStmt::StaticPropertySet { class_name, .. }
        | EvalStmt::StaticPropertyReferenceBind { class_name, .. }
        | EvalStmt::StaticPropertyArrayAppend { class_name, .. }
        | EvalStmt::StaticPropertyArraySet { class_name, .. }
        | EvalStmt::StaticPropertyIncDec { class_name, .. }
        | EvalStmt::UnsetStaticProperty { class_name, .. } => class_name,
        _ => return Ok(None),
    };
    let specialized = ensure_generic_class(name, None, "", context, scope, values)?;
    if *name == specialized { return Ok(None); }
    let mut concrete = stmt.clone();
    match &mut concrete {
        EvalStmt::StaticPropertySet { class_name, .. }
        | EvalStmt::StaticPropertyReferenceBind { class_name, .. }
        | EvalStmt::StaticPropertyArrayAppend { class_name, .. }
        | EvalStmt::StaticPropertyArraySet { class_name, .. }
        | EvalStmt::StaticPropertyIncDec { class_name, .. }
        | EvalStmt::UnsetStaticProperty { class_name, .. } => *class_name = specialized,
        _ => unreachable!("only static storage statements reach specialization"),
    }
    Ok(Some(concrete))
}
