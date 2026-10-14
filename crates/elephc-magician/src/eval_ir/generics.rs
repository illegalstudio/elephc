//! Purpose:
//! Retains generic declaration syntax and its lexical namespace for lazy specialization.
//!
//! Called from:
//! - Eval parser, persistent context registry, and interpreter generic dispatch.
//!
//! Key details:
//! - Tokens preserve literal bytes, source lines, and namespace imports across cached scripts.
//! - The prototype supplies argument metadata without executing a template body.

use super::*;
use crate::lexer::Token;
use crate::parser::state::NamespaceImports;
use elephc_generics::{Parameter, Type};

/// A function or class-like template awaiting concrete type arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalGenericTemplate {
    pub(crate) name: String,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) prototype: Box<EvalStmt>,
    pub(crate) tokens: Vec<Token>,
    pub(crate) substitutions: Vec<(usize, String)>,
    pub(crate) header_range: (usize, usize),
    pub(crate) constructor_parameters: Vec<Parameter>,
    pub(crate) namespace: String,
    pub(crate) imports: NamespaceImports,
    pub(crate) name_index: usize,
    pub(crate) native_syntax: bool,
}

/// Converts retained PHP type metadata into the shared generic type vocabulary.
pub(crate) fn generic_type(ty: &EvalParameterType) -> Type {
    let mut types = ty.variants().iter().map(|variant| match variant {
        EvalParameterTypeVariant::Generic(ty) => ty.clone(),
        EvalParameterTypeVariant::Class(name) => Type::atom(name),
        EvalParameterTypeVariant::Array => Type::atom("array"),
        EvalParameterTypeVariant::Bool => Type::atom("bool"),
        EvalParameterTypeVariant::Callable => Type::atom("callable"),
        EvalParameterTypeVariant::Float => Type::atom("float"),
        EvalParameterTypeVariant::Int => Type::atom("int"),
        EvalParameterTypeVariant::Iterable => Type::atom("iterable"),
        EvalParameterTypeVariant::Mixed => Type::atom("mixed"),
        EvalParameterTypeVariant::Never => Type::atom("never"),
        EvalParameterTypeVariant::Object => Type::atom("object"),
        EvalParameterTypeVariant::String => Type::atom("string"),
        EvalParameterTypeVariant::Void => Type::atom("void"),
    }).collect::<Vec<_>>();
    if ty.allows_null() && types.len() == 1 && types[0] != Type::atom("mixed") {
        return Type::Nullable(Box::new(types.remove(0)));
    }
    if ty.allows_null() { types.push(Type::atom("null")); }
    if types.len() == 1 { return types.remove(0); }
    if ty.is_intersection() { Type::Intersection(types) } else { Type::Union(types) }
}

/// Builds ordinary PHP metadata or a retained structural generic type.
pub(crate) fn parameter_type(ty: &Type) -> EvalParameterType {
    match ty {
        Type::Nullable(inner) => {
            let inner = parameter_type(inner);
            EvalParameterType::new(inner.variants().to_vec(), true)
        }
        Type::Union(types) | Type::Intersection(types) => {
            let atoms = types.iter().flat_map(|ty| parameter_type(ty).variants().to_vec()).collect();
            if matches!(ty, Type::Intersection(_)) { EvalParameterType::intersection(atoms) }
            else { EvalParameterType::new(atoms, types.iter().any(|ty| *ty == Type::atom("null"))) }
        }
        Type::Atom(name) => {
            let variant = match name.as_str() {
                "false" => EvalParameterTypeVariant::Generic(Type::atom("false")),
                "array" => EvalParameterTypeVariant::Array, "bool" => EvalParameterTypeVariant::Bool,
                "callable" => EvalParameterTypeVariant::Callable, "float" => EvalParameterTypeVariant::Float,
                "int" => EvalParameterTypeVariant::Int, "iterable" => EvalParameterTypeVariant::Iterable,
                "mixed" => EvalParameterTypeVariant::Mixed, "never" => EvalParameterTypeVariant::Never,
                "object" => EvalParameterTypeVariant::Object, "string" => EvalParameterTypeVariant::String,
                "void" => EvalParameterTypeVariant::Void,
                "null" => return EvalParameterType::new(Vec::new(), true),
                _ => EvalParameterTypeVariant::Class(name.clone()),
            };
            EvalParameterType::new(vec![variant], false)
        }
        _ => EvalParameterType::new(vec![EvalParameterTypeVariant::Generic(ty.clone())], false),
    }
}
