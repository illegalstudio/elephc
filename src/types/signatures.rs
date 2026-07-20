//! Purpose:
//! Defines function signature metadata for user functions, builtins, closures, and callable aliases.
//! Stores parameter names, defaults, variadics, by-reference behavior, and return contracts used by call planning.
//!
//! Called from:
//! - `crate::types::checker::functions`
//! - `crate::types::call_args`
//!
//! Key details:
//! - Builtin signatures must match PHP so named arguments, first-class callables, and mutation semantics stay coherent.

use crate::parser::ast::{AttributeGroup, Expr, ExprKind, TypeExpr};
use crate::span::Span;

use super::PhpType;

#[derive(Debug, Clone, PartialEq)]
/// Metadata for a callable's parameter and return type contract.
///
/// Used by call planning, named-argument resolution, first-class callables,
/// and type inference. Builtin signatures must match PHP for coherence with
/// named arguments, callable aliases, and mutation semantics.
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub param_type_exprs: Vec<Option<TypeExpr>>,
    pub param_attributes: Vec<Vec<AttributeGroup>>,
    pub defaults: Vec<Option<Expr>>,
    pub return_type: PhpType,
    pub declared_return: bool,
    /// `true` when declared with `function &f()` / `fn &()` — the function returns a
    /// reference (alias) to the returned lvalue rather than a copy.
    pub by_ref_return: bool,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
    pub variadic: Option<String>,
    /// `Some(message)` if the declaration carried PHP 8.4 `#[\Deprecated]`.
    /// `Some("")` indicates the attribute was present without an explicit
    /// reason. `None` means the function/method is not deprecated.
    pub deprecation: Option<String>,
}

/// Upgrades a variadic signature for use as a first-class callable.
///
/// If the variadic parameter is not already typed as `Array`, upgrades it to
/// `Array<Mixed>`. Non-variadic signatures are returned unchanged.
///
/// Called from:
/// - first-class callable lowering in codegen
pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return sig.clone();
    };

    let mut wrapper_sig = sig.clone();
    if let Some((name, ty)) = wrapper_sig.params.last_mut() {
        if name == variadic_name {
            if !matches!(ty, PhpType::Array(_)) {
                *ty = PhpType::Array(Box::new(PhpType::Mixed));
            }
            return wrapper_sig;
        }
    }

    let variadic_index = wrapper_sig.params.len();
    let variadic_type_expr = if wrapper_sig.param_type_exprs.len() > variadic_index {
        wrapper_sig.param_type_exprs.remove(variadic_index)
    } else {
        None
    };
    let variadic_attributes = if wrapper_sig.param_attributes.len() > variadic_index {
        wrapper_sig.param_attributes.remove(variadic_index)
    } else {
        Vec::new()
    };
    let variadic_ref = if wrapper_sig.ref_params.len() > variadic_index {
        wrapper_sig.ref_params.remove(variadic_index)
    } else {
        false
    };
    let variadic_declared = if wrapper_sig.declared_params.len() > variadic_index {
        wrapper_sig.declared_params.remove(variadic_index)
    } else {
        false
    };

    wrapper_sig.params.push((
        variadic_name.clone(),
        PhpType::Array(Box::new(PhpType::Mixed)),
    ));
    wrapper_sig.defaults.push(None);
    wrapper_sig.ref_params.push(variadic_ref);
    wrapper_sig.declared_params.push(variadic_declared);
    wrapper_sig.param_type_exprs.push(variadic_type_expr);
    wrapper_sig.param_attributes.push(variadic_attributes);
    wrapper_sig
}

/// Looks up a builtin function's canonical call signature.
///
/// Consults the builtin registry first, then the explicitly enumerated
/// compiler-resident language constructs. Returns `None` for untracked or
/// user-defined functions.
///
/// Called from:
/// - type checker builtin validation
/// - first-class callable builtin sig construction
/// - optimizer effect modeling for builtins
pub(crate) fn builtin_call_sig(name: &str) -> Option<FunctionSig> {
    crate::builtins::registry::function_sig(name)
        .or_else(|| compiler_resident_builtin_call_sig(name))
}

/// Returns call signatures for compiler-resident language constructs.
fn compiler_resident_builtin_call_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "eval" => Some(fixed(&["code"])),
        "empty" => Some(with_return_type(fixed(&["value"]), PhpType::Bool)),
        "isset" => Some(with_return_type(
            variadic(&["var"], "vars"),
            PhpType::Bool,
        )),
        "unset" => Some(with_return_type(
            variadic(&["var"], "vars"),
            PhpType::Void,
        )),
        "exit" | "die" => Some(with_return_type(
            optional(&["status"], 0, vec![int_lit(0)]),
            PhpType::Void,
        )),
        "buffer_new" => Some(fixed(&["length"])),
        _ => None,
    }
}

/// Sets the result type on a compiler-resident language-construct signature.
fn with_return_type(mut signature: FunctionSig, return_type: PhpType) -> FunctionSig {
    signature.return_type = return_type;
    signature
}

/// Returns the signature used when a builtin is accessed as a first-class callable.
///
/// The registry-derived signature is the single source used by direct and
/// first-class callable planning.
///
/// Called from:
/// - first-class callable lowering for builtin references
pub(crate) fn first_class_callable_builtin_sig(name: &str) -> Option<FunctionSig> {
    crate::builtins::registry::first_class_callable_sig(name)
}

/// Constructs a signature with all parameters required (no defaults).
fn fixed(params: &[&str]) -> FunctionSig {
    make_sig(params, vec![None; params.len()], None)
}

/// Constructs a signature with some trailing parameters optional.
///
/// `required` indicates how many leading params are mandatory; the rest receive
/// defaults from `optional_defaults` (mapped positionally). Defaults are padded
/// with `None` if fewer are provided than total params.
fn optional(params: &[&str], required: usize, optional_defaults: Vec<Expr>) -> FunctionSig {
    let mut defaults = vec![None; required];
    defaults.extend(optional_defaults.into_iter().map(Some));
    while defaults.len() < params.len() {
        defaults.push(None);
    }
    make_sig(params, defaults, None)
}

/// Constructs a variadic signature — trailing param collects excess arguments as an array.
///
/// `regular_params` lists the fixed parameters; `variadic_name` names the trailing
/// variadic parameter. The variadic param starts as an empty `array` default.
fn variadic(regular_params: &[&str], variadic_name: &str) -> FunctionSig {
    let mut params = regular_params.to_vec();
    params.push(variadic_name);
    let mut defaults = vec![None; regular_params.len()];
    defaults.push(Some(Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy())));
    make_sig(&params, defaults, Some(variadic_name))
}

/// Low-level `FunctionSig` constructor from raw parts.
///
/// Assembles params as `Mixed` types, sets all other fields from arguments,
/// and defaults `deprecation` to `None`.
fn make_sig(params: &[&str], defaults: Vec<Option<Expr>>, variadic: Option<&str>) -> FunctionSig {
    FunctionSig {
        params: params
            .iter()
            .map(|name| ((*name).to_string(), PhpType::Mixed))
            .collect(),
        param_type_exprs: vec![None; params.len()],
        param_attributes: vec![Vec::new(); params.len()],
        defaults,
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false; params.len()],
        declared_params: vec![false; params.len()],
        variadic: variadic.map(str::to_string),
        deprecation: None,
    }
}

/// Constructs an `i64` literal expression for use in default parameter values.
fn int_lit(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), Span::dummy())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Computes the callable signature metadata for variadic.
    fn variadic_sig(params: Vec<(String, PhpType)>) -> FunctionSig {
        FunctionSig {
            defaults: vec![None; params.len()],
            param_type_exprs: vec![None; params.len()],
            param_attributes: vec![Vec::new(); params.len()],
            return_type: PhpType::Mixed,
            declared_return: false,
            by_ref_return: false,
            ref_params: vec![false; params.len()],
            declared_params: vec![false; params.len()],
            params,
            variadic: Some("values".to_string()),
            deprecation: None,
        }
    }

    /// Builds the parameter metadata for callable wrapper sig retypes existing non array variadic.
    #[test]
    fn callable_wrapper_sig_retypes_existing_non_array_variadic_param() {
        let sig = variadic_sig(vec![
            ("format".to_string(), PhpType::Str),
            ("values".to_string(), PhpType::Mixed),
        ]);

        let wrapper_sig = callable_wrapper_sig(&sig);

        assert_eq!(wrapper_sig.params.len(), 2);
        assert_eq!(
            wrapper_sig.params[1],
            (
                "values".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )
        );
        assert_eq!(wrapper_sig.defaults.len(), 2);
        assert_eq!(wrapper_sig.ref_params.len(), 2);
        assert_eq!(wrapper_sig.declared_params.len(), 2);
    }

    /// Builds the parameter metadata for callable wrapper sig appends missing variadic.
    #[test]
    fn callable_wrapper_sig_appends_missing_variadic_param() {
        let sig = variadic_sig(vec![("format".to_string(), PhpType::Str)]);

        let wrapper_sig = callable_wrapper_sig(&sig);

        assert_eq!(wrapper_sig.params.len(), 2);
        assert_eq!(
            wrapper_sig.params[1],
            (
                "values".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )
        );
        assert_eq!(wrapper_sig.defaults.len(), 2);
        assert_eq!(wrapper_sig.ref_params.len(), 2);
        assert_eq!(wrapper_sig.declared_params.len(), 2);
    }

    /// Verifies compiler-resident constructs expose their concrete EIR result types.
    #[test]
    fn compiler_resident_constructs_have_precise_return_types() {
        assert_eq!(
            builtin_call_sig("empty").expect("empty signature").return_type,
            PhpType::Bool,
        );
        assert_eq!(
            builtin_call_sig("isset").expect("isset signature").return_type,
            PhpType::Bool,
        );
        assert_eq!(
            builtin_call_sig("unset").expect("unset signature").return_type,
            PhpType::Void,
        );
        assert_eq!(
            builtin_call_sig("exit").expect("exit signature").return_type,
            PhpType::Void,
        );
    }
}
