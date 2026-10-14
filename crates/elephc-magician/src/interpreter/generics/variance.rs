//! Purpose:
//! Validates generic variance promises and relates compatible object specializations.
//!
//! Called from:
//! - Template registration and structural argument/return validation.
//!
//! Key details:
//! - Constructors and private members are outside the widened public surface.
//! - Widening preserves AOT's storage rule: only object type arguments can differ.

use super::*;

/// Returns whether a type argument is an object reference in AOT storage.
pub(super) fn object_type(ty: &Type) -> bool {
    match ty {
        Type::Apply(name, _) => name != "array" && name != "buffer" && name != "ptr",
        Type::Atom(name) => !["int", "float", "string", "bool", "false", "null", "void", "never", "mixed", "array", "callable", "iterable", "object", "ptr"].contains(&name.as_str()),
        _ => false,
    }
}

/// Separates constructor identity slots from a generic class's public type arguments.
pub(super) fn generic_class_arguments<'a>(name: &str, args: &'a [Type], context: &ElephcEvalContext) -> &'a [Type] {
    if let Some(template) = context.generic_template(name) {
        let count = template.parameters.len();
        if count != 0 && !template.constructor_parameters.is_empty()
            && args.len() == count + template.constructor_parameters.len() { return &args[..count]; }
    }
    args
}

/// Removes constructor identity slots recursively before matching a declared generic shape.
pub(super) fn generic_inference_type(ty: &Type, context: &ElephcEvalContext) -> Type {
    match ty {
        Type::Apply(name, args) => Type::Apply(name.clone(), generic_class_arguments(name, args, context).iter().map(|ty| generic_inference_type(ty, context)).collect()),
        Type::Nullable(inner) => Type::Nullable(Box::new(generic_inference_type(inner, context))),
        Type::Union(types) => Type::Union(types.iter().map(|ty| generic_inference_type(ty, context)).collect()),
        Type::Intersection(types) => Type::Intersection(types.iter().map(|ty| generic_inference_type(ty, context)).collect()),
        Type::Callable(params, ret) => Type::Callable(params.iter().map(|ty| generic_inference_type(ty, context)).collect(), Box::new(generic_inference_type(ret, context))),
        _ => ty.clone(),
    }
}

/// Relates retained signature types using dynamic hierarchy metadata.
pub(in crate::interpreter) fn generic_signature_accepts(actual: &Type, expected: &Type, context: &ElephcEvalContext) -> bool {
    signature_accepts(actual, expected, context, &mut std::collections::HashSet::new())
}

/// Traverses substituted ancestors without requiring their specializations to be registered first.
fn signature_accepts(actual: &Type, expected: &Type, context: &ElephcEvalContext, seen: &mut std::collections::HashSet<(String, String)>) -> bool {
    if actual == expected || object_type(actual) && object_type(expected) && actual.to_string().eq_ignore_ascii_case(&expected.to_string()) { return true; }
    if !seen.insert((actual.to_string(), expected.to_string())) || seen.len() > 256 || actual.depth() > elephc_generics::MAX_TYPE_ARGUMENT_DEPTH { return false; }
    if context.class_is_a(&actual.to_string(), &expected.to_string(), false) { return true; }
    if signature_ancestors(actual, context).iter().any(|ancestor| signature_accepts(ancestor, expected, context, seen)) { return true; }
    if let (Type::Atom(actual), Type::Atom(expected)) = (actual, expected) {
        return actual.eq_ignore_ascii_case(expected) || context.class_is_a(actual, expected, false);
    }
    let (Type::Apply(actual_name, actual_args), Type::Apply(expected_name, expected_args)) = (actual, expected) else {
        return context.class_is_a(&actual.to_string(), &expected.to_string(), false);
    };
    let Some(template) = context.generic_template(actual_name) else { return false; };
    let actual_args = generic_class_arguments(actual_name, actual_args, context);
    let expected_args = generic_class_arguments(expected_name, expected_args, context);
    actual_name.eq_ignore_ascii_case(expected_name) && actual_args.len() == expected_args.len()
        && template.parameters.len() == actual_args.len()
        && template.parameters.iter().zip(actual_args.iter().zip(expected_args)).all(|(param, (actual, expected))| {
            if actual == expected { return true; }
            if !object_type(actual) || !object_type(expected) { return false; }
            match param.variance {
                1 => generic_signature_accepts(actual, expected, context),
                -1 => generic_signature_accepts(expected, actual, context),
                _ => false,
            }
        })
}

/// Projects a class or interface's declared relations through its concrete template bindings.
fn signature_ancestors(actual: &Type, context: &ElephcEvalContext) -> Vec<Type> {
    let (name, args) = match actual { Type::Atom(name) => (name, &[][..]), Type::Apply(name, args) => (name, generic_class_arguments(name, args, context)), _ => return Vec::new() };
    let mut relations = Vec::new();
    let mut bindings = Vec::new();
    if let Some(template) = context.generic_template(name) {
        bindings = template.parameters.iter().map(|param| param.name.clone()).zip(args.iter().cloned()).collect();
        match template.prototype.as_ref() {
            EvalStmt::ClassDecl(class) => relations.extend(class.parent().into_iter().chain(class.interfaces().iter().map(String::as_str))),
            EvalStmt::InterfaceDecl(interface) => relations.extend(interface.parents().iter().map(String::as_str)),
            _ => {}
        }
    } else if let Some(class) = context.class(&actual.to_string()) {
        relations.extend(class.parent().into_iter().chain(class.interfaces().iter().map(String::as_str)));
    } else if let Some(interface) = context.interface(&actual.to_string()) {
        relations.extend(interface.parents().iter().map(String::as_str));
    }
    relations.into_iter().filter_map(|relation| crate::parser::generics::parse_type_string(relation).ok()).map(|ty| ty.substitute(&bindings)).collect()
}

/// Checks whether any occurrence of a slot violates its declared polarity.
fn violates(ty: &Type, slot: &str, variance: i8, polarity: i8, context: &ElephcEvalContext) -> bool {
    match ty {
        Type::Atom(name) => name == slot && variance != polarity,
        Type::Nullable(inner) => violates(inner, slot, variance, polarity, context),
        Type::Union(types) | Type::Intersection(types) => types.iter().any(|ty| violates(ty, slot, variance, polarity, context)),
        Type::Callable(params, ret) => params.iter().any(|ty| violates(ty, slot, variance, -polarity, context))
            || violates(ret, slot, variance, polarity, context),
        Type::Apply(name, args) => {
            let template = context.generic_template(name);
            args.iter().enumerate().any(|(index, ty)| {
                let nested = if name == "array" || name == "buffer" { polarity }
                    else { polarity * template.and_then(|template| template.parameters.get(index)).map_or(0, |param| param.variance) };
                violates(ty, slot, variance, nested, context)
            })
        }
    }
}

/// Checks one member type after substituting inherited template arguments.
fn check_member(ty: &EvalParameterType, slot: &str, variance: i8, polarity: i8, bindings: &Bindings, context: &ElephcEvalContext) -> bool {
    !violates(&generic_type(ty).substitute(bindings), slot, variance, polarity, context)
}

/// Shields method-local slots before substituting the enclosing class or trait's arguments.
fn check_method(method: &crate::eval_ir::EvalClassMethod, slot: &str, variance: i8, bindings: &Bindings, context: &ElephcEvalContext) -> bool {
    let mut bindings = bindings.clone();
    if let Some(template) = method.generic_template() {
        for param in &template.parameters {
            bindings.retain(|(name, _)| name != &param.name);
            bindings.push((param.name.clone(), Type::atom(format!("@method_{}", param.name))));
        }
    }
    method.parameter_types().iter().flatten().all(|ty| check_member(ty, slot, variance, -1, &bindings, context))
        && method.return_type().is_none_or(|ty| check_member(ty, slot, variance, 1, &bindings, context))
}

/// One containing trait or class's method import policy, applied from inner to outer scope.
#[derive(Clone)]
struct TraitImport {
    name: String,
    adaptations: Vec<crate::eval_ir::EvalTraitAdaptation>,
    overrides: std::collections::HashSet<String>,
}

/// Validates used trait templates with the same visibility, aliases, and overrides as composition.
fn verify_trait_uses(
    traits: &[String], adaptations: &[crate::eval_ir::EvalTraitAdaptation], methods: &[crate::eval_ir::EvalClassMethod],
    slot: &str, variance: i8, bindings: &Bindings, context: &ElephcEvalContext, depth: usize, imports: &[TraitImport],
) -> bool {
    let adaptations = concrete_trait_adaptations(adaptations, traits);
    for relation in traits {
        let Ok(Type::Apply(name, arguments)) = crate::parser::generics::parse_type_string(relation) else { continue; };
        let Some(template) = context.generic_template(&name) else { continue; };
        let bindings = template.parameters.iter().map(|param| param.name.clone()).zip(arguments.iter().map(|ty| ty.substitute(bindings))).collect();
        let import = TraitImport {
            name: relation.clone(), adaptations: adaptations.clone(),
            overrides: methods.iter().map(|method| method.name().to_ascii_lowercase()).collect(),
        };
        let imports = std::iter::once(import).chain(imports.iter().cloned()).collect::<Vec<_>>();
        if !verify_slot(&template.prototype, slot, variance, &bindings, context, depth + 1, &imports) { return false; }
    }
    true
}

/// Resolves the effective callable surface without changing the retained generic prototype.
fn trait_method_surface(methods: &[crate::eval_ir::EvalClassMethod], imports: &[TraitImport]) -> Option<Vec<crate::eval_ir::EvalClassMethod>> {
    let mut methods = methods.to_vec();
    for import in imports {
        let adaptations = import.adaptations.iter().filter(|adaptation| match adaptation {
            crate::eval_ir::EvalTraitAdaptation::Alias { method, .. } => methods.iter().any(|candidate| candidate.name().eq_ignore_ascii_case(method)),
            _ => true,
        }).cloned().collect::<Vec<_>>();
        let trait_decl = crate::eval_ir::EvalTrait::new(import.name.clone(), Vec::new(), methods);
        methods = Vec::new();
        crate::interpreter::statements::append_eval_trait_methods(
            &trait_decl, &adaptations, &import.overrides, &mut std::collections::HashSet::new(), &mut methods,
        ).ok()?;
    }
    Some(methods)
}

/// Validates own and inherited public member types for one marked slot.
fn verify_slot(declaration: &EvalStmt, slot: &str, variance: i8, bindings: &Bindings, context: &ElephcEvalContext, depth: usize, imports: &[TraitImport]) -> bool {
    if depth > 64 { return false; }
    let mut inherited = Vec::new();
    match declaration {
        EvalStmt::ClassDecl(class) => {
            for property in class.properties().iter().filter(|property| property.visibility() != EvalVisibility::Private) {
                if property.property_type().is_some_and(|ty| !check_member(ty, slot, variance, if property.is_readonly() { 1 } else { 0 }, bindings, context)) { return false; }
            }
            for method in class.methods().iter().filter(|method| method.visibility() != EvalVisibility::Private && !method.name().eq_ignore_ascii_case("__construct")) {
                if !check_method(method, slot, variance, bindings, context) { return false; }
            }
            inherited.extend(class.parent().into_iter().chain(class.interfaces().iter().map(String::as_str)));
            if !verify_trait_uses(class.traits(), class.trait_adaptations(), class.methods(), slot, variance, bindings, context, depth, imports) { return false; }
        }
        EvalStmt::InterfaceDecl(interface) => {
            for property in interface.properties() {
                if property.property_type().is_some_and(|ty| !check_member(ty, slot, variance, if property.requires_set() { 0 } else { 1 }, bindings, context)) { return false; }
            }
            for method in interface.methods() {
                if method.parameter_types().iter().flatten().any(|ty| !check_member(ty, slot, variance, -1, bindings, context))
                    || method.return_type().is_some_and(|ty| !check_member(ty, slot, variance, 1, bindings, context)) { return false; }
            }
            inherited.extend(interface.parents().iter().map(String::as_str));
        }
        EvalStmt::TraitDecl(trait_decl) => {
            for property in trait_decl.properties().iter().filter(|property| property.visibility() != EvalVisibility::Private) {
                if property.property_type().is_some_and(|ty| !check_member(ty, slot, variance, if property.is_readonly() { 1 } else { 0 }, bindings, context)) { return false; }
            }
            let Some(methods) = trait_method_surface(trait_decl.methods(), imports) else { return false; };
            for method in methods.iter().filter(|method| method.visibility() != EvalVisibility::Private && !method.name().eq_ignore_ascii_case("__construct")) {
                if !check_method(method, slot, variance, bindings, context) { return false; }
            }
            if !verify_trait_uses(trait_decl.traits(), trait_decl.trait_adaptations(), trait_decl.methods(), slot, variance, bindings, context, depth, imports) { return false; }
        }
        _ => return true,
    }
    for relation in inherited {
        let Ok(Type::Apply(name, arguments)) = crate::parser::generics::parse_type_string(relation) else { continue; };
        let Some(parent) = context.generic_template(&name) else { continue; };
        let parent_bindings = parent.parameters.iter().map(|param| param.name.clone()).zip(arguments.iter().map(|ty| ty.substitute(bindings))).collect();
        if !verify_slot(&parent.prototype, slot, variance, &parent_bindings, context, depth + 1, &[]) { return false; }
    }
    true
}

/// Rejects invalid variance promises before any template body can execute.
pub(in crate::interpreter) fn verify_generic_variance(template: &EvalGenericTemplate, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    for param in &template.parameters {
        if param.variance != 0 && !verify_slot(&template.prototype, &param.name, param.variance, &Vec::new(), context, 0, &[]) {
            return eval_throw_type_error(&format!("Generic '{}' uses type parameter <{}> in a position forbidden by its variance", template.name, param.name), context, values);
        }
    }
    Ok(())
}
