//! Purpose:
//! Adapts compiler type syntax to the generic binding contract shared with Magician.
//!
//! Called from:
//! - `super::infer_bindings_with_args`, written argument binding, and unification.
//!
//! Key details:
//! - Compiler-only pointer and buffer spellings remain syntax terms; runtime support is separate.

use crate::names::Name;
use crate::parser::ast::{TypeExpr, TypeParam};
use elephc_generics::{Parameter, Type};

/// Converts compiler type syntax without losing nested generic arguments.
pub(super) fn to_shared(ty: &TypeExpr) -> Type {
    match ty {
        TypeExpr::Array(inner) => Type::Apply("array".into(), vec![to_shared(inner)]),
        TypeExpr::AssocArray { key, value } => Type::Apply("array".into(), vec![to_shared(key), to_shared(value)]),
        TypeExpr::Ptr(Some(name)) => Type::Apply("ptr".into(), vec![Type::atom(name.as_str())]),
        TypeExpr::Buffer(inner) => Type::Apply("buffer".into(), vec![to_shared(inner)]),
        TypeExpr::GenericClass { name, args } => Type::Apply(name.as_str().into(), args.iter().map(to_shared).collect()),
        TypeExpr::Nullable(inner) => Type::Nullable(Box::new(to_shared(inner))),
        TypeExpr::Union(types) => Type::Union(types.iter().map(to_shared).collect()),
        TypeExpr::Intersection(types) => Type::Intersection(types.iter().map(to_shared).collect()),
        TypeExpr::CallableSig { params, ret } => Type::Callable(params.iter().map(to_shared).collect(), Box::new(to_shared(ret))),
        _ => Type::atom(super::describe_type(ty)),
    }
}

/// Converts resolved shared syntax back to compiler type expressions.
pub(super) fn from_shared(ty: Type) -> TypeExpr {
    match ty {
        Type::Atom(name) => match name.as_str() {
            "int" => TypeExpr::Int, "float" => TypeExpr::Float, "string" => TypeExpr::Str,
            "bool" => TypeExpr::Bool, "false" => TypeExpr::False, "null" => TypeExpr::Void,
            "ptr" => TypeExpr::Ptr(None), "never" => TypeExpr::Never, "iterable" => TypeExpr::Iterable,
            _ => TypeExpr::Named(Name::unqualified(&name)),
        },
        Type::Apply(name, mut args) => match (name.as_str(), args.len()) {
            ("array", 1) => TypeExpr::Array(Box::new(from_shared(args.remove(0)))),
            ("array", 2) => TypeExpr::AssocArray { key: Box::new(from_shared(args.remove(0))), value: Box::new(from_shared(args.remove(0))) },
            ("ptr", 1) => TypeExpr::Ptr(Some(Name::unqualified(&args.remove(0).to_string()))),
            ("buffer", 1) => TypeExpr::Buffer(Box::new(from_shared(args.remove(0)))),
            _ => TypeExpr::GenericClass { name: Name::unqualified(&name), args: args.into_iter().map(from_shared).collect() },
        },
        Type::Nullable(inner) => TypeExpr::Nullable(Box::new(from_shared(*inner))),
        Type::Union(types) => TypeExpr::Union(types.into_iter().map(from_shared).collect()),
        Type::Intersection(types) => TypeExpr::Intersection(types.into_iter().map(from_shared).collect()),
        Type::Callable(params, ret) => TypeExpr::CallableSig { params: params.into_iter().map(from_shared).collect(), ret: Box::new(from_shared(*ret)) },
    }
}

/// Converts declared slots while keeping bounds and dependent defaults intact.
pub(super) fn parameters(params: &[TypeParam]) -> Vec<Parameter> {
    params.iter().map(|param| Parameter { name: param.name.clone(), bound: param.bound.as_ref().map(to_shared), default: param.default.as_ref().map(to_shared), variance: 0 }).collect()
}

/// Maps shared inference failures to the existing compiler diagnostic contract.
pub(super) fn infer_error(error: elephc_generics::Error) -> super::InferError {
    match error {
        elephc_generics::Error::Unconstrained(param) => super::InferError::Unconstrained(param),
        elephc_generics::Error::Conflict { param, first, second } => super::InferError::Conflict { param, first, second },
        elephc_generics::Error::TooDeep { param, ty } => super::InferError::TooDeep { param, ty },
        _ => unreachable!("inference never binds written arguments"),
    }
}
