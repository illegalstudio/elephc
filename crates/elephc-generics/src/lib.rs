//! Purpose:
//! Shared generic type terms, binding, defaults, and specialization limits for AOT and eval.
//!
//! Called from:
//! - Compiler generic inference and Magician template specialization.
//!
//! Key details:
//! - Binding conflicts are errors; absent evidence uses a declared default only.
//! - Defaults are substituted in declaration order and recursive type growth is bounded.

use std::fmt;

/// Maximum nesting of a concrete type argument, shared by both execution paths.
pub const MAX_TYPE_ARGUMENT_DEPTH: usize = 8;

/// Backend-independent type syntax used to bind generic parameters.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Atom(String),
    Apply(String, Vec<Type>),
    Nullable(Box<Type>),
    Union(Vec<Type>),
    Intersection(Vec<Type>),
    Callable(Vec<Type>, Box<Type>),
}

/// One declared generic slot, with its bound, default, and variance promise.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub bound: Option<Type>,
    pub default: Option<Type>,
    pub variance: i8,
}

/// Concrete type arguments in declaration order.
pub type Bindings = Vec<(String, Type)>;

/// A binding failure expressed without compiler or interpreter dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unconstrained(String),
    Conflict { param: String, first: String, second: String },
    TooDeep { param: String, ty: String },
    TooMany { written: usize, declared: usize },
    Unbound { param: String, written: usize },
}

impl Type {
    /// Constructs an ordinary type name.
    pub fn atom(name: impl Into<String>) -> Self { Self::Atom(name.into()) }

    /// Substitutes bound slots recursively through every type position.
    pub fn substitute(&self, bindings: &Bindings) -> Self {
        match self {
            Self::Atom(name) => bindings.iter().find(|(key, _)| key == name)
                .map(|(_, ty)| ty.clone()).unwrap_or_else(|| self.clone()),
            Self::Apply(name, args) => Self::Apply(name.clone(), args.iter().map(|ty| ty.substitute(bindings)).collect()),
            Self::Nullable(ty) => Self::Nullable(Box::new(ty.substitute(bindings))),
            Self::Union(types) => Self::Union(types.iter().map(|ty| ty.substitute(bindings)).collect()),
            Self::Intersection(types) => Self::Intersection(types.iter().map(|ty| ty.substitute(bindings)).collect()),
            Self::Callable(params, ret) => Self::Callable(params.iter().map(|ty| ty.substitute(bindings)).collect(), Box::new(ret.substitute(bindings))),
        }
    }

    /// Measures nesting before a specialization can recurse indefinitely.
    pub fn depth(&self) -> usize {
        match self {
            Self::Atom(_) => 1,
            Self::Nullable(ty) => 1 + ty.depth(),
            Self::Apply(_, types) | Self::Union(types) | Self::Intersection(types) => 1 + types.iter().map(Self::depth).max().unwrap_or(0),
            Self::Callable(params, ret) => 1 + params.iter().map(Self::depth).chain(std::iter::once(ret.depth())).max().unwrap_or(0),
        }
    }
}

impl fmt::Display for Type {
    /// Renders the same concrete spelling used for specialization identities.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Atom(name) => out.write_str(name),
            Self::Apply(name, args) => write!(out, "{}<{}>", name, joined(args, ", ")),
            Self::Nullable(ty) => write!(out, "?{ty}"),
            Self::Union(types) => out.write_str(&joined(types, "|")),
            Self::Intersection(types) => out.write_str(&joined(types, "&")),
            Self::Callable(params, ret) => write!(out, "callable({}): {ret}", joined(params, ", ")),
        }
    }
}

/// Renders a type sequence in source order.
fn joined(types: &[Type], separator: &str) -> String {
    types.iter().map(ToString::to_string).collect::<Vec<_>>().join(separator)
}

/// Unifies the shapes AOT generic inference understands, without guessing other shapes.
pub fn unify(names: &[String], declared: &Type, actual: &Type, bindings: &mut Bindings) -> Result<(), Error> {
    match declared {
        Type::Atom(name) if names.contains(name) => {
            if let Some((_, existing)) = bindings.iter().find(|(key, _)| key == name) {
                if existing != actual {
                    return Err(Error::Conflict { param: name.clone(), first: existing.to_string(), second: actual.to_string() });
                }
            } else { bindings.push((name.clone(), actual.clone())); }
        }
        Type::Nullable(inner) => unify(names, inner, actual, bindings)?,
        Type::Apply(name, args) => if let Type::Apply(other, actual_args) = actual {
            if name.eq_ignore_ascii_case(other) && args.len() == actual_args.len() {
                for (declared, actual) in args.iter().zip(actual_args) { unify(names, declared, actual, bindings)?; }
            }
        },
        _ => {}
    }
    Ok(())
}

/// Finishes inferred bindings, resolving dependent defaults and checking the depth limit.
pub fn finish(parameters: &[Parameter], inferred: &Bindings) -> Result<Bindings, Error> {
    let mut result = Vec::new();
    for param in parameters {
        let ty = inferred.iter().find(|(name, _)| name == &param.name).map(|(_, ty)| ty.clone())
            .or_else(|| param.default.as_ref().map(|ty| ty.substitute(&result)))
            .ok_or_else(|| Error::Unconstrained(param.name.clone()))?;
        if ty.depth() > MAX_TYPE_ARGUMENT_DEPTH {
            return Err(Error::TooDeep { param: param.name.clone(), ty: ty.to_string() });
        }
        result.push((param.name.clone(), ty));
    }
    Ok(result)
}

/// Binds written arguments, filling only declared defaults and never inferring the remainder.
pub fn written(parameters: &[Parameter], arguments: &[Type]) -> Result<Bindings, Error> {
    if arguments.len() > parameters.len() { return Err(Error::TooMany { written: arguments.len(), declared: parameters.len() }); }
    let mut bindings = Vec::new();
    for (index, param) in parameters.iter().enumerate() {
        let ty = arguments.get(index).cloned()
            .or_else(|| param.default.as_ref().map(|ty| ty.substitute(&bindings)))
            .ok_or_else(|| Error::Unbound { param: param.name.clone(), written: arguments.len() })?;
        bindings.push((param.name.clone(), ty));
    }
    Ok(bindings)
}

/// Strips opening, closing, and continuation markers from either PHPDoc layout.
pub fn strip_comment_markers(line: &str) -> String {
    line.trim().trim_start_matches("/**").trim_end_matches("*/").trim().trim_start_matches('*').trim().to_string()
}

/// Reads the variance promise from supported PHPStan/Psalm template tags.
pub fn strip_template_tag(line: &str) -> Option<(&str, i8)> {
    for (tag, variance) in [("@template-covariant ", 1), ("@template-contravariant ", -1), ("@template ", 0)] {
        if let Some(rest) = line.strip_prefix(tag) { return Some((rest, variance)); }
    }
    None
}
