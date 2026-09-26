//! Purpose:
//! Checks that a variance marker (`+T`, `-T`) is honoured by the template that declares it: a
//! covariant parameter may only be produced, a contravariant one may only be consumed.
//!
//! Called from:
//! - `crate::generics::monomorphize`, once per compile, right after the templates are collected.
//!
//! Key details:
//! - This is a property of the DECLARATION, not of any instantiation, so it runs once and before
//!   anything is spliced. A violation caught here names the member that breaks the promise; the
//!   same violation caught at a call site would name an instantiation the programmer never
//!   wrote.
//! - Polarity COMPOSES. `T` in a return type is produced, but `Sink<T>` in a return type, where
//!   `Sink` is contravariant in its parameter, consumes `T`. Walking the type and flipping at
//!   each contravariant slot is the whole of the check, and the reason it cannot be a simple
//!   "does this type mention T" test.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Stmt, StmtKind, TypeExpr, Variance, Visibility,
};
use crate::span::Span;

use super::classes::{template_key, Templates};

/// Where a type parameter sits relative to the boundary of its template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    /// Produced: a return type, or a property that can only be read.
    Out,
    /// Consumed: a parameter.
    In,
    /// Both, and therefore compatible with no marker: a writable property.
    Both,
}

impl Polarity {
    /// Flips `In` and `Out`, leaving `Both` alone.
    ///
    /// This is what makes the check compose: reaching `T` through a contravariant slot means the
    /// template consumes it even though the enclosing position produces.
    fn flipped(self) -> Self {
        match self {
            Polarity::Out => Polarity::In,
            Polarity::In => Polarity::Out,
            Polarity::Both => Polarity::Both,
        }
    }

    /// Returns whether a parameter declared `variance` may occur here.
    fn admits(self, variance: Variance) -> bool {
        match variance {
            Variance::Invariant => true,
            Variance::Covariant => self == Polarity::Out,
            Variance::Contravariant => self == Polarity::In,
        }
    }

    /// How the position reads in a diagnostic.
    fn describe(self) -> &'static str {
        match self {
            Polarity::Out => "an output position",
            Polarity::In => "an input position",
            Polarity::Both => "a position that is both read and written",
        }
    }
}

/// Rejects every template whose body breaks the promise its variance markers make.
pub fn verify(templates: &Templates) -> Result<(), CompileError> {
    // Variance is rare and the walk is not free, so the common program pays one map lookup.
    let markers = marker_table(templates);
    if markers.is_empty() {
        return Ok(());
    }
    for template in templates.declarations() {
        for param in &template.type_params {
            if param.variance == Variance::Invariant {
                continue;
            }
            verify_parameter(
                &template.declared_name,
                &param.name,
                param.variance,
                &template.declaration,
                &markers,
                templates,
                &[],
                0,
            )?;
        }
    }
    Ok(())
}

/// The variance each template declares, by template key, for composing through a mention.
fn marker_table(templates: &Templates) -> HashMap<String, Vec<Variance>> {
    templates
        .declarations()
        .filter(|template| {
            template
                .type_params
                .iter()
                .any(|param| param.variance != Variance::Invariant)
        })
        .map(|template| {
            (
                template_key(&template.declared_name),
                template
                    .type_params
                    .iter()
                    .map(|param| param.variance)
                    .collect(),
            )
        })
        .collect()
}

/// Walks one declaration's members, rejecting the first occurrence the marker does not admit.
#[allow(clippy::too_many_arguments)]
fn verify_parameter(
    declared_name: &str,
    param: &str,
    variance: Variance,
    declaration: &Stmt,
    markers: &HashMap<String, Vec<Variance>>,
    templates: &Templates,
    bindings: &[(String, TypeExpr)],
    depth: usize,
) -> Result<(), CompileError> {
    // A cycle in the inheritance clauses is a separate diagnostic's job; this walk only has to
    // stop.
    if depth > MAX_INHERITANCE_DEPTH {
        return Ok(());
    }
    let (properties, methods) = match &declaration.kind {
        StmtKind::ClassDecl {
            properties,
            methods,
            ..
        }
        | StmtKind::InterfaceDecl {
            properties,
            methods,
            ..
        } => (properties.as_slice(), methods.as_slice()),
        _ => return Ok(()),
    };
    for property in properties {
        check_property(declared_name, param, variance, property, markers, bindings)?;
    }
    for method in methods {
        check_method(declared_name, param, variance, method, markers, bindings)?;
    }
    // INHERITED members keep the promise too, and this is the half that was missing. After
    // monomorphization `class Box<+T> extends Holder<T>` is `Box<Dog> extends Holder<Dog>`, whose
    // flattened surface includes `Holder`'s `set(T)`. That member consumes `T` exactly the way a
    // `+T` forbids, but it is written on the parent, so reading only the marked class's own
    // members certified a marker the class does not keep — and a widened reference then wrote a
    // `Cat` into a `Box<Dog>`.
    for (inherited, arguments) in inherited_templates(declaration) {
        let Some(parent) = templates.by_key(&template_key(inherited.as_str())) else {
            continue;
        };
        // The child's arguments are written in the CHILD's parameters, so they are substituted
        // through the bindings already in force before becoming the parent's.
        let parent_bindings: Vec<(String, TypeExpr)> = parent
            .type_params
            .iter()
            .map(|p| p.name.clone())
            .zip(
                arguments
                    .iter()
                    .map(|argument| argument.substitute_type_params(bindings)),
            )
            .collect();
        verify_parameter(
            declared_name,
            param,
            variance,
            &parent.declaration,
            markers,
            templates,
            &parent_bindings,
            depth + 1,
        )?;
    }
    Ok(())
}

/// How deep the inheritance walk follows a chain of generic parents.
const MAX_INHERITANCE_DEPTH: usize = 16;

/// The generic parents and interfaces a declaration inherits, with the arguments written on each.
///
/// A bare name carries no arguments and cannot mention a type parameter, so it is skipped: only
/// an inheritance that passes something down can break the promise.
fn inherited_templates(declaration: &Stmt) -> Vec<(crate::names::Name, Vec<TypeExpr>)> {
    let mut out = Vec::new();
    match &declaration.kind {
        StmtKind::ClassDecl {
            generics,
            extends,
            implements,
            ..
        } => {
            let Some(generics) = generics else { return out };
            if let Some(parent) = extends {
                if !generics.extends_args.is_empty() {
                    out.push((parent.clone(), generics.extends_args.clone()));
                }
            }
            for (name, args) in implements.iter().zip(generics.interface_args.iter()) {
                if !args.is_empty() {
                    out.push((name.clone(), args.clone()));
                }
            }
        }
        StmtKind::InterfaceDecl {
            generics, extends, ..
        } => {
            let Some(generics) = generics else { return out };
            for (name, args) in extends.iter().zip(generics.interface_args.iter()) {
                if !args.is_empty() {
                    out.push((name.clone(), args.clone()));
                }
            }
        }
        _ => {}
    }
    out
}

/// A property constrains variance only when it is part of the template's external surface.
fn check_property(
    declared_name: &str,
    param: &str,
    variance: Variance,
    property: &ClassProperty,
    markers: &HashMap<String, Vec<Variance>>,
    bindings: &[(String, TypeExpr)],
) -> Result<(), CompileError> {
    if property.visibility == Visibility::Private {
        return Ok(());
    }
    let Some(ty) = &property.type_expr else {
        return Ok(());
    };
    // An INHERITED member is written in the parent's parameters; substituting the child's
    // arguments is what turns `set(U)` on `Holder<U>` into the `set(T)` the child really has.
    let ty = &ty.substitute_type_params(bindings);
    // A property that can be written from outside is read AND written through the widened
    // reference, so it admits no marker at all. `readonly` removes the write and with it the
    // reason: that is precisely the case `+T` exists to serve.
    let polarity = if property.readonly {
        Polarity::Out
    } else {
        Polarity::Both
    };
    check_type(
        declared_name,
        param,
        variance,
        ty,
        polarity,
        markers,
        property.span,
        &format!("property ${}", property.name),
    )
}

/// A method's parameters consume and its return produces.
fn check_method(
    declared_name: &str,
    param: &str,
    variance: Variance,
    method: &ClassMethod,
    markers: &HashMap<String, Vec<Variance>>,
    bindings: &[(String, TypeExpr)],
) -> Result<(), CompileError> {
    if method.visibility == Visibility::Private {
        return Ok(());
    }
    // A CONSTRUCTOR is exempt, and not as a convenience: a constructor cannot be reached through
    // a widened reference. `new Box<Animal>(...)` names the instantiation it builds, so the
    // widening this check protects never applies to it. Counting constructor parameters would
    // make `+T` impossible for every container that stores a `T`, which is all of them.
    if method.name.eq_ignore_ascii_case("__construct") {
        return Ok(());
    }
    let member = format!("method {}()", method.name);
    for (name, declared, _, _) in &method.params {
        if let Some(ty) = declared {
            let ty = &ty.substitute_type_params(bindings);
            check_type(
                declared_name,
                param,
                variance,
                ty,
                Polarity::In,
                markers,
                method.span,
                &format!("{} parameter ${}", member, name),
            )?;
        }
    }
    if let Some(ty) = &method.variadic_type.as_ref().map(|t| t.substitute_type_params(bindings)) {
        check_type(
            declared_name,
            param,
            variance,
            ty,
            Polarity::In,
            markers,
            method.span,
            &format!("{} variadic parameter", member),
        )?;
    }
    if let Some(ty) = &method.return_type.as_ref().map(|t| t.substitute_type_params(bindings)) {
        check_type(
            declared_name,
            param,
            variance,
            ty,
            Polarity::Out,
            markers,
            method.span,
            &format!("{} return type", member),
        )?;
    }
    Ok(())
}

/// Rejects the first occurrence of `param` inside `ty` that `variance` does not admit.
#[allow(clippy::too_many_arguments)]
fn check_type(
    declared_name: &str,
    param: &str,
    variance: Variance,
    ty: &TypeExpr,
    polarity: Polarity,
    markers: &HashMap<String, Vec<Variance>>,
    span: Span,
    member: &str,
) -> Result<(), CompileError> {
    let mut found = None;
    collect_occurrence(param, ty, polarity, variance, markers, &mut found);
    let Some(found) = found else { return Ok(()) };
    if found.admits(variance) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!(
            "Type parameter '{}{}' of '{}' appears in {} in {}; a {} parameter may only appear in {}",
            variance.marker(),
            param,
            declared_name,
            found.describe(),
            member,
            variance_noun(variance),
            required_position(variance),
        ),
    ))
}

/// Records the first VIOLATING occurrence of `param` in `ty`, if any.
///
/// Violating, not merely first. Recording whichever occurrence came first made coverage depend on
/// source order: in `T|Sink<T>` under `+T`, member zero is `T` at `Out`, which the marker
/// admits — and the walk stopped there, never reaching the contravariant `Sink<T>` that follows.
/// The identical type written `Sink<T>|T` was rejected. One violating occurrence is one too many,
/// which is what this now implements rather than merely claims.
fn collect_occurrence(
    param: &str,
    ty: &TypeExpr,
    polarity: Polarity,
    variance: Variance,
    markers: &HashMap<String, Vec<Variance>>,
    found: &mut Option<Polarity>,
) {
    if found.is_some() {
        return;
    }
    match ty {
        TypeExpr::Named(name) if name.as_str() == param => {
            if !polarity.admits(variance) {
                *found = Some(polarity);
            }
        }
        TypeExpr::Named(_) => {}
        // A PHP array is a value: assigning one copies it, so a `T` read out through an
        // `array<T>` cannot be written back into the template's own storage. The element
        // therefore keeps the enclosing polarity rather than collapsing to `Both`, which is what
        // lets `all(): array<T>` stay legal under `+T`.
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            collect_occurrence(param, inner, polarity, variance, markers, found)
        }
        TypeExpr::AssocArray { key, value } => {
            collect_occurrence(param, key, polarity, variance, markers, found);
            collect_occurrence(param, value, polarity, variance, markers, found);
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            for member in members {
                collect_occurrence(param, member, polarity, variance, markers, found);
            }
        }
        // A CALLABLE flips its parameters and keeps its return, and the rule falls out of who
        // calls whom. When a template PRODUCES a `callable(A): B`, its consumer calls it: the
        // template must accept whatever `A` the consumer passes (an input) and must supply the
        // `B` (an output). When a template CONSUMES one — `map(callable(T): U $f)` — the template
        // itself calls it: it supplies the `A` (an output) and receives the `B` (an input).
        // Either way the parameters invert the enclosing polarity and the return keeps it.
        //
        // Without this arm the `_` fallback swallowed the whole type, so `Sink<-T>` could hand
        // its `T` to a callback and `Box<+T>` could receive one, both silently.
        TypeExpr::CallableSig { params, ret } => {
            for argument in params {
                collect_occurrence(param, argument, polarity.flipped(), variance, markers, found);
            }
            collect_occurrence(param, ret, polarity, variance, markers, found);
        }
        // Reaching `T` through another template's slot composes with that slot's own variance:
        // `Sink<T>` in a return type CONSUMES `T` when `Sink` is contravariant. A template with
        // no marker is invariant in every slot, so anything inside it is `Both`.
        TypeExpr::GenericClass { name, args } => {
            let slots = markers.get(&template_key(name.as_str()));
            for (index, arg) in args.iter().enumerate() {
                let slot = slots
                    .and_then(|slots| slots.get(index).copied())
                    .unwrap_or(Variance::Invariant);
                let nested = match slot {
                    Variance::Invariant => Polarity::Both,
                    Variance::Covariant => polarity,
                    Variance::Contravariant => polarity.flipped(),
                };
                collect_occurrence(param, arg, nested, variance, markers, found);
            }
        }
        _ => {}
    }
}

/// The marker's name, for the diagnostic.
fn variance_noun(variance: Variance) -> &'static str {
    match variance {
        Variance::Invariant => "invariant",
        Variance::Covariant => "covariant",
        Variance::Contravariant => "contravariant",
    }
}

/// The only position the marker admits, for the diagnostic.
fn required_position(variance: Variance) -> &'static str {
    match variance {
        Variance::Invariant => "any position",
        Variance::Covariant => "an output position",
        Variance::Contravariant => "an input position",
    }
}
