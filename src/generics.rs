//! Purpose:
//! Monomorphization support for generic function declarations: binding type parameters to the
//! concrete types a call site supplies, naming the resulting instantiation, and cloning the
//! template's declaration with those bindings substituted.
//!
//! Called from:
//! - `crate::types::checker` (inference and instantiation at a call site)
//! - `crate::pipeline` (splicing instantiated declarations back into the program)
//!
//! Key details:
//! - A generic declaration is a TEMPLATE and is never lowered as written. Every reachable
//!   instantiation becomes its own ordinary monomorphic function, so codegen, the ABI, and the
//!   optimizer never learn that generics exist.
//! - The instantiated name embeds its type arguments (`identity<int>`). `names::mangle_fqn` is
//!   total and injective and already escapes `<` and `>`, so no new symbol scheme is needed,
//!   and the name cannot collide with a user function: the parser rejects `<` in a name.
//!
//! - Every compile path must go through [`monomorphize`], not just the CLI pipeline: a caller
//!   that checks and lowers by hand leaves the templates in place and hands lowering a call to
//!   a function that does not exist.

pub mod classes;
pub mod methods;
mod splice;
pub mod variance;

pub use splice::{
    instantiation_roots, splice_instantiations, strip_templates, substitute_in_body,
};

use crate::span::Span;

/// What the checker needs to know about generics beyond the program itself.
///
/// Two halves of one handover. `class_type_argument_bounds` is work the instantiating pass
/// could not finish — deciding whether `User` satisfies `T : Entity` needs the class table.
/// `class_templates` is work it cannot start — `new Box(5)` writes no arguments, so only the
/// checker's view of the argument types can determine them.
///
/// One struct rather than two parameters because both travel the same path, through the same
/// closure, to the same call; adding the third the next feature needs should not re-thread
/// every caller again.
#[derive(Debug, Default)]
pub struct GenericContext {
    pub class_type_argument_bounds: Vec<classes::BoundObligation>,
    pub class_templates: Vec<classes::TemplateSignature>,
    /// Every generic METHOD the program declares, rebuilt each round.
    ///
    /// A method template rides along when its class is instantiated, so `Box<int>` only gains
    /// `map<U>` once `Box<T>` has been instantiated — collecting once at the start would miss
    /// every method on an inferred class.
    pub method_templates:
        std::collections::HashMap<(String, String), methods::MethodTemplate>,
}

/// The upper bound on monomorphization rounds.
///
/// A backstop, not the primary defence: polymorphic recursion is caught by
/// [`MAX_TYPE_ARGUMENT_DEPTH`] at the instantiation that creates it, which can name the type.
/// Exceeding this bound means something grew that depth did not describe, so it is an ERROR —
/// compiling on regardless would emit a program missing instantiations its call sites need.
pub const MAX_INSTANTIATION_ROUNDS: usize = 16;

/// Type-checks `program` to a fixpoint, instantiating every generic call site along the way,
/// and returns the program with templates replaced by their instantiations.
///
/// Each round records the instantiations its call sites asked for, splices one declaration per
/// entry, and checks again — because an instantiated body can itself call a template. The loop
/// settles once a round adds nothing, which is guaranteed for any program without polymorphic
/// recursion, since instantiations are keyed by name.
///
/// Every compile path must go through this, not just the CLI pipeline: a caller that checks
/// and lowers by hand would leave the templates in place and hand lowering a call to a function
/// that does not exist.
pub fn monomorphize<F>(
    program: crate::parser::ast::Program,
    mut check: F,
) -> Result<(crate::parser::ast::Program, crate::types::CheckResult), crate::errors::CompileError>
where
    F: FnMut(
        &crate::parser::ast::Program,
        &GenericContext,
    ) -> Result<crate::types::CheckResult, crate::errors::CompileError>,
{
    // Generic CLASSES are resolved first and entirely without the checker: their type arguments
    // are written, not inferred, so instantiating them is pure syntax. Doing it here rather than
    // in the pipeline is what keeps the two in step — an instantiated FUNCTION body can name
    // `Box<T>`, which only becomes the concrete `Box<int>` once the checker has bound `T`.
    let class_templates = classes::collect(&program);
    // A variance marker is a promise about the DECLARATION, so it is checked once, here, before
    // anything is spliced. Checking it per instantiation would name a class the programmer never
    // wrote, and checking it after splicing would be too late to name the member at fault.
    variance::verify(&class_templates)?;
    let first = classes::instantiate(
        program,
        &class_templates,
        &classes::InferredConstructions::default(),
    )?;
    let program = first.program;
    let obligations = first.obligations;
    // Warnings the INSTANTIATING pass produced — a bare template name read as an instantiation —
    // travel with the checker's, so a compile reports both in one place.
    let mut instantiation_warnings = first.warnings;
    let mut context = GenericContext {
        class_type_argument_bounds: obligations,
        // Carried for the checker's own half of the job: `new Box(5)` writes no type arguments,
        // so only the checker can determine them, and it needs the constructors to do it.
        class_templates: class_templates.signatures(),
        method_templates: methods::collect(&program),
    };
    let mut program = program;
    let mut rounds = 0usize;
    // Warnings are collected ACROSS rounds, not taken from the last one.
    //
    // Each round re-checks the whole program, so a warning about ordinary code is produced
    // again every time and the union is the same set the last round would give. What differs
    // is a warning only an EARLY round can produce: `new Box(anything())` warns that the
    // inferred `mixed` erases the instantiation, and by the next round that construction reads
    // `new Box<mixed>(…)` — an ordinary construction with nothing left to warn about. Taking
    // the last round's list silently dropped exactly the warnings this fixpoint exists to
    // produce.
    let mut collected_warnings: Vec<crate::errors::CompileWarning> = Vec::new();
    let mut check_result = loop {
        let result = check(&program, &context)?;
        for warning in &result.warnings {
            if !collected_warnings
                .iter()
                .any(|seen| seen.span == warning.span && seen.message == warning.message)
            {
                collected_warnings.push(warning.clone());
            }
        }
        // Two kinds of work can come back from a check: FUNCTION instantiations, which the
        // checker resolved from call-site types, and CLASS instantiations, which it resolved
        // from constructor arguments at a `new` with no written type arguments. Either one
        // means the program is about to grow, so the round is not the last.
        let inferred = classes::InferredConstructions {
            requested: result.requested_class_instantiations.clone(),
            sites: result.generic_new_sites.clone(),
        };
        let class_work = !inferred.requested.is_empty();
        // A third kind of work: METHOD instantiations, which the checker resolved from the
        // argument types at a call to a generic method. Like the other two, it means the program
        // is about to grow, so the round is not the last.
        let method_work = methods::Requested {
            instantiations: result.requested_method_instantiations.clone(),
            sites: result.generic_method_sites.clone(),
        };
        if result.requested_instantiations.is_empty() && !class_work && method_work.is_empty() {
            break result;
        }
        let (spliced, added) = splice_instantiations(program, &result.requested_instantiations);
        program = spliced;
        let (with_methods, method_added) =
            methods::instantiate(program, &context.method_templates, &method_work)?;
        program = with_methods;
        if added == 0 && method_added == 0 && !class_work {
            break result;
        }
        // The bodies just spliced in were written against `T` and are concrete now, so a
        // `Box<T>` among them has become a `Box<int>` that needs its own class. This is also
        // where the checker's `new Box(5)` inferences are adopted: the class it asked for is
        // spliced and the construction is renamed to it.
        let round = classes::instantiate(program, &class_templates, &inferred)?;
        program = round.program;
        context.class_type_argument_bounds.extend(round.obligations);
        for warning in round.warnings {
            if !instantiation_warnings
                .iter()
                .any(|seen| seen.span == warning.span && seen.message == warning.message)
            {
                instantiation_warnings.push(warning);
            }
        }
        // Rebuilt from the program as it now stands: the classes just spliced in carry their
        // template's generic methods, and a call on one of them can only find them from here.
        context.method_templates = methods::collect(&program);
        rounds += 1;
        if rounds >= MAX_INSTANTIATION_ROUNDS {
            return Err(crate::errors::CompileError::new(
                Span::dummy(),
                &format!(
                    "Generic instantiation did not settle after {} rounds. Every round added a \
                     new instantiation, so some generic function instantiates itself at an \
                     ever-changing type",
                    MAX_INSTANTIATION_ROUNDS
                ),
            ));
        }
    };
    for warning in instantiation_warnings {
        if !collected_warnings
            .iter()
            .any(|seen| seen.span == warning.span && seen.message == warning.message)
        {
            collected_warnings.push(warning);
        }
    }
    check_result.warnings = collected_warnings;
    // A template is not a function: its annotations name types with no representation, so it
    // must never reach lowering. Every instantiation the program reaches now exists.
    let program = strip_templates(program);
    // A generic METHOD is a template too, and lives on a class that itself survives. Its body
    // names types with no representation — `new Box<U>` — so leaving it in reaches lowering and
    // panics there. Every call the program makes has its own instantiated method by now.
    let program = methods::strip_templates(program);
    Ok((program, check_result))
}

use crate::names::Name;
use crate::parser::ast::{TypeExpr, TypeParam};
use crate::types::PhpType;

/// Returns the `TypeExpr` that denotes `ty`, or `None` when the type has no source spelling.
///
/// Substitution rewrites the template's ANNOTATIONS, which are `TypeExpr`s, so a call site's
/// inferred `PhpType` has to be expressed back in that vocabulary. Types with no surface
/// syntax — the codegen-internal representations, and the pointer/resource/packed families
/// that a generic parameter has no way to name — return `None`, which makes the call site
/// decline to instantiate rather than invent a spelling.
pub fn type_expr_for_php_type(ty: &PhpType) -> Option<TypeExpr> {
    Some(match ty {
        PhpType::Int => TypeExpr::Int,
        PhpType::Float => TypeExpr::Float,
        PhpType::Str => TypeExpr::Str,
        PhpType::Bool => TypeExpr::Bool,
        PhpType::False => TypeExpr::False,
        PhpType::Void => TypeExpr::Void,
        PhpType::Never => TypeExpr::Never,
        PhpType::Iterable => TypeExpr::Iterable,
        PhpType::Mixed => TypeExpr::Named(Name::unqualified("mixed")),
        PhpType::Callable => TypeExpr::Named(Name::unqualified("callable")),
        PhpType::Object(name) => TypeExpr::Named(Name::unqualified(name)),
        PhpType::Array(elem) => TypeExpr::Array(Box::new(type_expr_for_php_type(elem)?)),
        PhpType::AssocArray { key, value } => TypeExpr::AssocArray {
            key: Box::new(type_expr_for_php_type(key)?),
            value: Box::new(type_expr_for_php_type(value)?),
        },
        PhpType::Union(members) => TypeExpr::Union(
            members
                .iter()
                .map(type_expr_for_php_type)
                .collect::<Option<Vec<_>>>()?,
        ),
        PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => return None,
    })
}

/// One resolved type argument list, in the template's declaration order.
pub type Bindings = Vec<(String, TypeExpr)>;

/// Why a call site could not be monomorphized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferError {
    /// A type parameter appears in no parameter position the call site constrains, so nothing
    /// determines it. Explicit type arguments (a turbofish) would be the way out.
    Unconstrained(String),
    /// Two parameter positions bind the same type parameter to different types.
    Conflict {
        param: String,
        first: String,
        second: String,
    },
    /// The inferred type has no source spelling, so the template cannot be rewritten for it.
    Unspellable { param: String, ty: String },
    /// The type argument is nested deeper than [`MAX_TYPE_ARGUMENT_DEPTH`].
    ///
    /// This is what polymorphic recursion looks like from the inside: `f<T>()` calling
    /// `f<array<T>>()` instantiates `f<int>`, then `f<array<int>>`, then
    /// `f<array<array<int>>>`, each one a legitimate instantiation, forever. Depth is the
    /// signal because it is what actually grows; the instantiation COUNT does not distinguish
    /// a runaway from a template a program simply uses with many types.
    TooDeep { param: String, ty: String },
}

/// How deeply a type argument may nest before it is treated as polymorphic recursion.
///
/// Eight is far past anything a program writes by hand — `array<array<array<int>>>` is three —
/// while still terminating a runaway in a handful of rounds.
pub const MAX_TYPE_ARGUMENT_DEPTH: usize = 8;

/// Returns how deeply `ty` nests, counting the outermost constructor as depth 1.
fn type_expr_depth(ty: &TypeExpr) -> usize {
    match ty {
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            1 + type_expr_depth(inner)
        }
        TypeExpr::AssocArray { key, value } => {
            1 + type_expr_depth(key).max(type_expr_depth(value))
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            1 + members.iter().map(type_expr_depth).max().unwrap_or(0)
        }
        _ => 1,
    }
}

/// Infers a type argument for every type parameter from the actual argument types.
///
/// Unification walks each declared parameter type against the corresponding argument type and
/// binds a type parameter wherever the declaration names one. The traversal deliberately
/// mirrors the shapes substitution can produce, so anything `substitute_type_params` rewrites
/// is also something this can read back:
///
/// - `T $x` against `int` binds `T = int`
/// - `array<T> $xs` against `array<string>` binds `T = string`
/// - `array<K, V> $m` against `array<string, int>` binds `K = string` and `V = int`
/// - `?T $x` against `int` binds `T = int` — the nullable wrapper is the declaration's, not
///   the type argument's
///
/// Every other shape contributes no binding rather than guessing, so an unconstrained type
/// parameter is reported instead of silently becoming `mixed`.
pub fn infer_bindings(
    type_params: &[TypeParam],
    declared_params: &[Option<TypeExpr>],
    actual_types: &[PhpType],
) -> Result<Bindings, InferError> {
    infer_bindings_with_args(type_params, declared_params, actual_types, &[])
}

/// Infers type arguments, also reading the argument EXPRESSIONS where the type cannot say enough.
///
/// One case needs this and it is the whole reason `TypeExpr::CallableSig` exists. A closure's
/// type is `PhpType::Callable` and carries no signature, so a `callable(T): U` parameter matched
/// against it binds nothing — the information is in the closure's own declared types, which live
/// on the expression. Every other parameter is decided by its type alone, exactly as before.
pub fn infer_bindings_with_args(
    type_params: &[TypeParam],
    declared_params: &[Option<TypeExpr>],
    actual_types: &[PhpType],
    args: &[crate::parser::ast::Expr],
) -> Result<Bindings, InferError> {
    let names: Vec<String> = type_params.iter().map(|param| param.name.clone()).collect();
    let mut bound: Vec<(String, TypeExpr)> = Vec::new();
    for (index, declared) in declared_params.iter().enumerate() {
        let (Some(TypeExpr::CallableSig { params, ret }), Some(arg)) =
            (declared.as_ref(), args.get(index))
        else {
            continue;
        };
        bind_from_closure(&names, params, ret, arg, &mut bound)?;
    }
    for (declared, actual) in declared_params.iter().zip(actual_types.iter()) {
        let Some(declared) = declared else { continue };
        if !declared.mentions_type_param(&names) {
            continue;
        }
        unify(&names, declared, actual, &mut bound)?;
    }
    let mut ordered: Bindings = Vec::with_capacity(type_params.len());
    for param in type_params {
        // A default is what an unconstrained parameter falls back to. Without one, nothing
        // determines the parameter and guessing `mixed` would give up exactly the storage the
        // annotation exists to pin, so it stays an error.
        let ty = match bound.iter().find(|(name, _)| name == &param.name) {
            Some((_, ty)) => ty.clone(),
            // A default may NAME an earlier type parameter (`<T, U = T>`), which is a type only
            // once that parameter is bound. `ordered` already holds every parameter declared
            // before this one — they are filled in declaration order — so substituting through it
            // turns `U = T` into `U = int` instead of leaving a name nothing downstream resolves.
            None => param
                .default
                .clone()
                .ok_or_else(|| InferError::Unconstrained(param.name.clone()))?
                .substitute_type_params(&ordered),
        };
        if type_expr_depth(&ty) > MAX_TYPE_ARGUMENT_DEPTH {
            return Err(InferError::TooDeep {
                param: param.name.clone(),
                ty: describe(&ty),
            });
        }
        ordered.push((param.name.clone(), ty));
    }
    Ok(ordered)
}

/// Places a call's arguments at the declaration positions they bind to.
///
/// Inference pairs a declared parameter with the argument at the SAME INDEX, so a call's source
/// order has to become the declaration's first: a named argument binds by name and may be written
/// anywhere. A positional argument takes the next declared slot; a named one takes the slot whose
/// parameter it names, unwrapped to its value so inference sees the expression rather than the
/// `NamedArg` wrapper.
///
/// Anything left over — a name the declaration does not have, or a surplus positional bound for a
/// variadic — keeps its source order after the declared slots, which is the order the variadic
/// collects them in. A slot no argument fills stays `None`: its default is not an argument, and
/// inference must not read a type from one.
pub fn arguments_in_declaration_order(
    param_names: &[String],
    args: &[crate::parser::ast::Expr],
) -> Vec<Option<crate::parser::ast::Expr>> {
    let mut slots: Vec<Option<crate::parser::ast::Expr>> = vec![None; param_names.len()];
    let mut surplus: Vec<crate::parser::ast::Expr> = Vec::new();
    let mut next_positional = 0usize;
    for arg in args {
        if let crate::parser::ast::ExprKind::NamedArg { name, value } = &arg.kind {
            if let Some(index) = param_names.iter().position(|param| param == name) {
                slots[index] = Some((**value).clone());
                continue;
            }
            surplus.push((**value).clone());
            continue;
        }
        if next_positional < slots.len() {
            slots[next_positional] = Some(arg.clone());
            next_positional += 1;
            continue;
        }
        surplus.push(arg.clone());
    }
    slots.extend(surplus.into_iter().map(Some));
    slots
}

/// Returns the declared type at one ordered argument position.
///
/// Past the declared parameters sit the arguments the VARIADIC collects, and its declared element
/// type is a binding position like any other: `function first<T>(T ...$xs)` determines `T` from
/// the first of them.
pub fn declared_type_at(
    index: usize,
    declared_count: usize,
    param_types: &[Option<TypeExpr>],
    variadic_type: Option<&TypeExpr>,
) -> Option<TypeExpr> {
    if index < declared_count {
        param_types.get(index).cloned().flatten()
    } else {
        variadic_type.cloned()
    }
}

/// Binds the type parameters a `callable(T): U` mentions, from the closure the call passes.
///
/// Only a closure LITERAL can answer: a callable named by a string or held in a variable has no
/// declared types at the call site. An argument that is not one simply binds nothing here and
/// falls through to the ordinary type-driven pass, which reports the honest
/// `Unconstrained` if that leaves a parameter undetermined.
///
/// A closure parameter with no hint is skipped rather than treated as `mixed`: inferring `mixed`
/// would silently give up the storage the annotation exists to pin, which is the same reason an
/// unconstrained parameter is an error instead of a guess.
fn bind_from_closure(
    names: &[String],
    declared_params: &[TypeExpr],
    declared_ret: &TypeExpr,
    arg: &crate::parser::ast::Expr,
    bound: &mut Vec<(String, TypeExpr)>,
) -> Result<(), InferError> {
    let crate::parser::ast::ExprKind::Closure {
        params,
        return_type,
        ..
    } = &arg.kind
    else {
        return Ok(());
    };
    for (declared, (_, actual, _, _)) in declared_params.iter().zip(params.iter()) {
        let Some(actual) = actual else { continue };
        if declared.mentions_type_param(names) {
            unify_type_exprs(names, declared, actual, bound)?;
        }
    }
    if let Some(actual) = return_type {
        if declared_ret.mentions_type_param(names) {
            unify_type_exprs(names, declared_ret, actual, bound)?;
        }
    }
    Ok(())
}

/// Walks one declared parameter type against its argument type, recording bindings.
fn unify(
    type_params: &[String],
    declared: &TypeExpr,
    actual: &PhpType,
    bound: &mut Vec<(String, TypeExpr)>,
) -> Result<(), InferError> {
    match declared {
        TypeExpr::Named(name) if type_params.iter().any(|param| param == name.as_str()) => {
            let param = name.as_str().to_string();
            let Some(spelled) = type_expr_for_php_type(actual) else {
                return Err(InferError::Unspellable {
                    param,
                    ty: actual.to_string(),
                });
            };
            if let Some((_, existing)) = bound.iter().find(|(existing, _)| existing == &param) {
                if existing != &spelled {
                    return Err(InferError::Conflict {
                        param,
                        first: describe(existing),
                        second: actual.to_string(),
                    });
                }
                return Ok(());
            }
            bound.push((param, spelled));
            Ok(())
        }
        // The declaration's own nullable wrapper is not part of the type argument: `?T` called
        // with an `int` binds `T = int`, and the parameter stays `?int` after substitution.
        TypeExpr::Nullable(inner) => unify(type_params, inner, actual, bound),
        TypeExpr::Array(inner) => match actual {
            PhpType::Array(elem) => unify(type_params, inner, elem, bound),
            _ => Ok(()),
        },
        TypeExpr::AssocArray { key, value } => match actual {
            PhpType::AssocArray {
                key: actual_key,
                value: actual_value,
            } => {
                unify(type_params, key, actual_key, bound)?;
                unify(type_params, value, actual_value, bound)
            }
            _ => Ok(()),
        },
        // `Box<T>` against an argument of type `Box<int>`. The ACTUAL type is an ordinary
        // object by now — `generics::classes` instantiated it long before the checker ran —
        // but its name still carries the arguments it was instantiated with, so decoding the
        // name recovers exactly the structure this declaration has to be matched against.
        //
        // The decode is a round trip through the language's own type grammar, not string
        // surgery: `instantiated_name` writes the name with `describe`, whose output is valid
        // type syntax by construction, so `parse_type_expr` is its inverse.
        TypeExpr::GenericClass { name, args } => {
            let PhpType::Object(object) = actual else {
                return Ok(());
            };
            let Some(TypeExpr::GenericClass {
                name: actual_name,
                args: actual_args,
            }) = instantiated_type(object)
            else {
                return Ok(());
            };
            // A different class entirely says nothing about this declaration's parameters, and
            // the checker reports the argument mismatch on its own.
            if !actual_name.as_str().eq_ignore_ascii_case(name.as_str())
                || actual_args.len() != args.len()
            {
                return Ok(());
            }
            for (declared, actual) in args.iter().zip(actual_args.iter()) {
                unify_type_exprs(type_params, declared, actual, bound)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Unifies a declared type against a type ARGUMENT that is itself a type expression.
///
/// The ordinary [`unify`] matches against a `PhpType`, which is what a call site's argument
/// carries. Decoding an instantiated class name gives back `TypeExpr`s instead, so the same
/// walk is needed one vocabulary over.
fn unify_type_exprs(
    type_params: &[String],
    declared: &TypeExpr,
    actual: &TypeExpr,
    bound: &mut Vec<(String, TypeExpr)>,
) -> Result<(), InferError> {
    match declared {
        TypeExpr::Named(name) if type_params.iter().any(|param| param == name.as_str()) => {
            let param = name.as_str().to_string();
            if let Some((_, existing)) = bound.iter().find(|(existing, _)| existing == &param) {
                if existing != actual {
                    return Err(InferError::Conflict {
                        param,
                        first: describe(existing),
                        second: describe(actual),
                    });
                }
                return Ok(());
            }
            bound.push((param, actual.clone()));
            Ok(())
        }
        TypeExpr::Nullable(inner) => unify_type_exprs(type_params, inner, actual, bound),
        TypeExpr::Array(inner) => match actual {
            TypeExpr::Array(actual) => unify_type_exprs(type_params, inner, actual, bound),
            _ => Ok(()),
        },
        TypeExpr::AssocArray { key, value } => match actual {
            TypeExpr::AssocArray {
                key: actual_key,
                value: actual_value,
            } => {
                unify_type_exprs(type_params, key, actual_key, bound)?;
                unify_type_exprs(type_params, value, actual_value, bound)
            }
            _ => Ok(()),
        },
        TypeExpr::GenericClass { name, args } => match actual {
            TypeExpr::GenericClass {
                name: actual_name,
                args: actual_args,
            } if actual_name.as_str().eq_ignore_ascii_case(name.as_str())
                && actual_args.len() == args.len() =>
            {
                for (declared, actual) in args.iter().zip(actual_args.iter()) {
                    unify_type_exprs(type_params, declared, actual, bound)?;
                }
                Ok(())
            }
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

/// Decodes an instantiated class name back into the type it denotes.
///
/// `Box<Box<int>>` is a CLASS NAME after instantiation, and also a complete description of what
/// that class holds — `instantiated_name` built it with `describe`, whose output is valid type
/// syntax. Reading it back through `parser::stmt::parse_type_expr`, the language's own grammar,
/// is what lets a generic function infer `T` from an argument whose class is generic, without a
/// side table mapping instantiations to their arguments.
///
/// Returns `None` for an ordinary class name, which has no `<` and therefore nothing to decode.
pub fn instantiated_type(name: &str) -> Option<TypeExpr> {
    if !name.contains('<') {
        return None;
    }
    let source = format!("<?php {};", name);
    let tokens = crate::lexer::tokenize(&source).ok()?;
    let mut pos = 1usize;
    let span = tokens.get(pos).map(|(_, meta)| meta.span)?;
    let parsed = crate::parser::stmt::parse_type_expr(&tokens, &mut pos, span).ok()?;
    // The whole name must be consumed, or a name this module did not write could decode to a
    // prefix of itself and bind a type parameter to the wrong argument.
    match tokens.get(pos).map(|(token, _)| token) {
        Some(crate::lexer::Token::Semicolon) => Some(parsed),
        _ => None,
    }
}

/// Renders a `TypeExpr` for a diagnostic, in the spelling a programmer would write.
///
/// Public under a fuller name for the passes outside this module that report a type they were
/// handed: the instantiated name and the diagnostic have to agree on how a type is spelled.
pub fn describe_type(ty: &TypeExpr) -> String {
    describe(ty)
}

/// Renders a `TypeExpr` for a diagnostic, in the spelling a programmer would write.
fn describe(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Str => "string".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::False => "false".to_string(),
        TypeExpr::Void => "null".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Iterable => "iterable".to_string(),
        TypeExpr::CallableSig { params, ret } => format!(
            "callable({}): {}",
            params.iter().map(describe).collect::<Vec<_>>().join(", "),
            describe(ret)
        ),
        TypeExpr::Named(name) => name.as_str().to_string(),
        TypeExpr::Array(inner) => format!("array<{}>", describe(inner)),
        TypeExpr::AssocArray { key, value } => {
            format!("array<{}, {}>", describe(key), describe(value))
        }
        TypeExpr::Buffer(inner) => format!("buffer<{}>", describe(inner)),
        TypeExpr::GenericClass { name, args } => format!(
            "{}<{}>",
            name.as_str(),
            args.iter().map(describe).collect::<Vec<_>>().join(", ")
        ),
        TypeExpr::Nullable(inner) => format!("?{}", describe(inner)),
        TypeExpr::Ptr(Some(name)) => format!("ptr<{}>", name.as_str()),
        TypeExpr::Ptr(None) => "ptr".to_string(),
        TypeExpr::Union(members) => members
            .iter()
            .map(describe)
            .collect::<Vec<_>>()
            .join("|"),
        TypeExpr::Intersection(members) => members
            .iter()
            .map(describe)
            .collect::<Vec<_>>()
            .join("&"),
    }
}

/// Why a WRITTEN type argument list does not bind a template's parameters.
#[derive(Debug, Clone, PartialEq)]
pub enum WrittenError {
    /// More arguments than the template declares parameters.
    TooMany { written: usize, declared: usize },
    /// A parameter past the written list, with no default to fall back on.
    Unbound { param: String, written: usize },
}

/// Binds type parameters from the arguments a CALL SITE wrote, filling the rest from defaults.
///
/// The written list is positional, like every other type argument list in the language, and it
/// may be shorter than the parameter list only where the remaining parameters declare defaults:
/// `pair<int>(1, "x")` against `pair<A, B = string>` binds `B` to `string`. Inferring the
/// remainder from the call's arguments instead would make one list half-written and
/// half-inferred, which is two rules for one syntax.
///
/// Shared by the function and method paths so both accept exactly the same lists; only the
/// wording of the diagnostic differs, and that belongs to the caller which knows what it is
/// resolving.
pub fn bindings_from_written_arguments(
    type_params: &[TypeParam],
    written: &[TypeExpr],
) -> Result<Bindings, WrittenError> {
    if written.len() > type_params.len() {
        return Err(WrittenError::TooMany {
            written: written.len(),
            declared: type_params.len(),
        });
    }
    let mut bindings = Bindings::new();
    for (index, param) in type_params.iter().enumerate() {
        let argument = match written.get(index) {
            Some(argument) => argument.clone(),
            // Same dependent default as the inferred path: `identity<int>` leaves `U` to its
            // default `T`, which is only a type once `T` is bound. The bindings built so far are
            // the earlier parameters, in declaration order.
            None => param
                .default
                .clone()
                .ok_or_else(|| WrittenError::Unbound {
                    param: param.name.clone(),
                    written: written.len(),
                })?
                .substitute_type_params(&bindings),
        };
        bindings.push((param.name.clone(), argument));
    }
    Ok(bindings)
}

/// Splits `map<string>` into its base name and its decoded type arguments.
///
/// The name IS the encoding — the same spelling `instantiated_name` produces — so decoding it
/// with the language's own type grammar is exact, nested arguments included. Returns `None` for
/// an ordinary name, which costs one `contains` on the common path.
pub fn split_written_instantiation(name: &str) -> Option<(String, Vec<TypeExpr>)> {
    if !name.contains('<') {
        return None;
    }
    match instantiated_type(name)? {
        TypeExpr::GenericClass { name, args } => Some((name.as_str().to_string(), args)),
        _ => None,
    }
}

/// Renders a WRITTEN type argument list that does not fit, for a function or a method.
///
/// Shared so the two surfaces answer the same way about the same list; `kind` is the only word
/// that differs, and it is what tells the reader which declaration to go and look at.
pub fn describe_written_error(
    kind: &str,
    name: &str,
    error: &WrittenError,
) -> String {
    match error {
        WrittenError::TooMany { written, declared } => format!(
            "Call to generic {} '{}' writes {} type argument{}, but it declares {}",
            kind,
            name,
            written,
            if *written == 1 { "" } else { "s" },
            declared
        ),
        WrittenError::Unbound { param, written } => format!(
            "Call to generic {} '{}' writes {} type argument{} but leaves <{}> unbound, and it \
             declares no default",
            kind,
            name,
            written,
            if *written == 1 { "" } else { "s" },
            param
        ),
    }
}

/// Returns the monomorphic name for one instantiation, e.g. `identity<int>`.
///
/// The type arguments are part of the name so two instantiations of the same template are two
/// distinct functions everywhere downstream — signature table, symbol, diagnostics. The
/// spelling is the programmer's, which keeps an error message about `identity<string>`
/// readable, and `names::mangle_fqn` turns it into a valid symbol without further help.
pub fn instantiated_name(base: &str, bindings: &[(String, TypeExpr)]) -> String {
    let args = bindings
        .iter()
        .map(|(_, ty)| describe(ty))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}<{}>", base, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Variance;

    fn param(ty: TypeExpr) -> Option<TypeExpr> {
        Some(ty)
    }

    /// A plain type parameter: no bound, no default.
    fn tp(name: &str) -> TypeParam {
        TypeParam {
            name: name.to_string(),
            bound: None,
            default: None,
            variance: Variance::Invariant,
        }
    }

    fn t(name: &str) -> TypeExpr {
        TypeExpr::Named(Name::unqualified(name))
    }

    #[test]
    fn binds_a_bare_type_parameter_from_the_argument() {
        let bindings = infer_bindings(
            &[tp("T")],
            &[param(t("T"))],
            &[PhpType::Int],
        )
        .expect("T is constrained by the only parameter");
        assert_eq!(bindings, vec![("T".to_string(), TypeExpr::Int)]);
    }

    #[test]
    fn binds_through_an_array_element_type() {
        let bindings = infer_bindings(
            &[tp("T")],
            &[param(TypeExpr::Array(Box::new(t("T"))))],
            &[PhpType::Array(Box::new(PhpType::Str))],
        )
        .expect("T is constrained by the element type");
        assert_eq!(bindings, vec![("T".to_string(), TypeExpr::Str)]);
    }

    #[test]
    fn binds_both_halves_of_an_associative_array() {
        let bindings = infer_bindings(
            &[tp("K"), tp("V")],
            &[param(TypeExpr::AssocArray {
                key: Box::new(t("K")),
                value: Box::new(t("V")),
            })],
            &[PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            }],
        )
        .expect("both halves are constrained");
        assert_eq!(
            bindings,
            vec![
                ("K".to_string(), TypeExpr::Str),
                ("V".to_string(), TypeExpr::Int),
            ]
        );
    }

    /// The declaration's own nullable wrapper is not part of the type argument.
    #[test]
    fn a_nullable_declaration_binds_the_inner_type() {
        let bindings = infer_bindings(
            &[tp("T")],
            &[param(TypeExpr::Nullable(Box::new(t("T"))))],
            &[PhpType::Int],
        )
        .expect("T is constrained through the nullable wrapper");
        assert_eq!(bindings, vec![("T".to_string(), TypeExpr::Int)]);
    }

    #[test]
    fn two_positions_disagreeing_is_a_conflict() {
        let err = infer_bindings(
            &[tp("T")],
            &[param(t("T")), param(t("T"))],
            &[PhpType::Int, PhpType::Str],
        )
        .expect_err("one parameter cannot be both int and string");
        assert!(matches!(err, InferError::Conflict { .. }));
    }

    #[test]
    fn two_positions_agreeing_is_fine() {
        let bindings = infer_bindings(
            &[tp("T")],
            &[param(t("T")), param(t("T"))],
            &[PhpType::Int, PhpType::Int],
        )
        .expect("both positions agree");
        assert_eq!(bindings, vec![("T".to_string(), TypeExpr::Int)]);
    }

    #[test]
    fn a_type_parameter_no_argument_mentions_is_unconstrained() {
        let err = infer_bindings(&[tp("T")], &[param(TypeExpr::Int)], &[PhpType::Int])
            .expect_err("nothing determines T");
        assert_eq!(err, InferError::Unconstrained("T".to_string()));
    }

    /// A type with no source spelling makes the call site decline rather than invent one.
    #[test]
    fn an_unspellable_argument_type_is_reported() {
        let err = infer_bindings(
            &[tp("T")],
            &[param(t("T"))],
            &[PhpType::Pointer(None)],
        )
        .expect_err("ptr has no generic spelling");
        assert!(matches!(err, InferError::Unspellable { .. }));
    }

    /// A default is what an unconstrained parameter falls back to, instead of erroring.
    #[test]
    fn an_unconstrained_parameter_falls_back_to_its_default() {
        let declared = TypeParam {
            name: "K".to_string(),
            bound: None,
            default: Some(TypeExpr::Str),
            variance: Variance::Invariant,
        };
        let bindings = infer_bindings(&[declared], &[param(TypeExpr::Int)], &[PhpType::Int])
            .expect("the default determines K");
        assert_eq!(bindings, vec![("K".to_string(), TypeExpr::Str)]);
    }

    /// An inferred argument still wins over the default.
    #[test]
    fn inference_beats_the_default() {
        let declared = TypeParam {
            name: "T".to_string(),
            bound: None,
            default: Some(TypeExpr::Str),
            variance: Variance::Invariant,
        };
        let bindings = infer_bindings(&[declared], &[param(t("T"))], &[PhpType::Int])
            .expect("T is constrained by the argument");
        assert_eq!(bindings, vec![("T".to_string(), TypeExpr::Int)]);
    }

    #[test]
    fn instantiated_names_are_distinct_and_readable() {
        let int = instantiated_name("identity", &[("T".to_string(), TypeExpr::Int)]);
        let string = instantiated_name("identity", &[("T".to_string(), TypeExpr::Str)]);
        assert_eq!(int, "identity<int>");
        assert_eq!(string, "identity<string>");
        assert_ne!(int, string);
    }

    #[test]
    fn instantiated_names_mangle_to_distinct_symbols() {
        let int = crate::names::mangle_fqn(&instantiated_name(
            "identity",
            &[("T".to_string(), TypeExpr::Int)],
        ));
        let string = crate::names::mangle_fqn(&instantiated_name(
            "identity",
            &[("T".to_string(), TypeExpr::Str)],
        ));
        assert_ne!(int, string);
        assert!(int.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }

    #[test]
    fn substitution_rewrites_every_position() {
        let bindings = vec![("T".to_string(), TypeExpr::Int)];
        assert_eq!(t("T").substitute_type_params(&bindings), TypeExpr::Int);
        assert_eq!(
            TypeExpr::Array(Box::new(t("T"))).substitute_type_params(&bindings),
            TypeExpr::Array(Box::new(TypeExpr::Int))
        );
        assert_eq!(
            TypeExpr::Nullable(Box::new(t("T"))).substitute_type_params(&bindings),
            TypeExpr::Nullable(Box::new(TypeExpr::Int))
        );
        // A name that is not a type parameter survives untouched.
        assert_eq!(t("Foo").substitute_type_params(&bindings), t("Foo"));
    }
}
