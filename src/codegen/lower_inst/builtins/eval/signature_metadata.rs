//! Purpose:
//! Checks bridge-compatible signatures and formats their PHP type metadata.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Variadic, by-reference, and return-type constraints remain conservative.

use super::*;

/// Returns true when a module function should expose metadata to eval fragments.
pub(super) fn function_has_eval_metadata(function: &Function) -> bool {
    !function.flags.is_main && !function.name.starts_with('_')
}

/// Returns true when eval can dispatch a native function through the generated bridge.
pub(super) fn function_signature_can_bridge_with_eval(function: &Function) -> bool {
    function
        .params
        .iter()
        .all(|param| !param.by_ref || eval_native_function_ref_param_supported(&param.php_type))
}

/// Returns true when a native function by-reference parameter can use eval bridge staging.
pub(super) fn eval_native_function_ref_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Int
            | PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Object(_)
            | PhpType::Str
    )
}

/// Returns true when eval can dispatch a native method through the generated bridge.
pub(super) fn method_signature_can_bridge_with_eval(signature: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(signature)
        && signature
            .params
            .iter()
            .all(|(_, ty)| eval_native_method_param_supported(ty))
        && eval_native_method_return_supported(&signature.return_type)
}

/// Returns true when eval can dispatch a native constructor through the generated bridge.
pub(super) fn constructor_signature_can_bridge_with_eval(signature: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(signature)
        && signature
            .params
            .iter()
            .all(|(_, ty)| eval_native_constructor_param_supported(ty))
}

/// Returns true when one native method argument type fits the eval method bridge.
pub(super) fn eval_native_method_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true when one native constructor argument type fits the eval bridge.
pub(super) fn eval_native_constructor_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true when one native method return type can be boxed back for eval.
pub(super) fn eval_native_method_return_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Void
            | PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
    )
}

/// Returns true when the indexed parameter is the signature's variadic slot.
pub(super) fn signature_param_is_variadic(signature: &FunctionSig, index: usize, param_name: &str) -> bool {
    signature.variadic.as_deref().is_some_and(|variadic| {
        variadic == param_name
            || signature
                .params
                .get(index)
                .is_some_and(|(name, _)| name == variadic)
    })
}

/// Returns generated type specs for declared native callable parameters.
pub(super) fn eval_native_callable_param_type_specs(signature: &FunctionSig) -> Vec<Option<String>> {
    signature
        .params
        .iter()
        .enumerate()
        .map(|(index, (_, php_type))| {
            if !signature
                .declared_params
                .get(index)
                .copied()
                .unwrap_or(false)
            {
                return None;
            }
            signature
                .param_type_exprs
                .get(index)
                .and_then(Option::as_ref)
                .and_then(eval_native_type_expr_spec)
                .or_else(|| eval_native_php_type_spec(php_type, false))
        })
        .collect()
}

/// Returns a generated type spec for a declared native callable return type.
pub(super) fn eval_native_callable_return_type_spec(signature: &FunctionSig) -> Option<String> {
    signature
        .declared_return
        .then(|| eval_native_php_type_spec(&signature.return_type, true))
        .flatten()
}

/// Formats one parsed PHP type expression for eval native metadata registration.
pub(super) fn eval_native_type_expr_spec(type_expr: &TypeExpr) -> Option<String> {
    match type_expr {
        TypeExpr::Int => Some("int".to_string()),
        TypeExpr::Float => Some("float".to_string()),
        TypeExpr::Bool => Some("bool".to_string()),
        TypeExpr::False => Some("false".to_string()),
        TypeExpr::Str => Some("string".to_string()),
        TypeExpr::Void => Some("null".to_string()),
        TypeExpr::Never => None,
        TypeExpr::Iterable => Some("iterable".to_string()),
        TypeExpr::Array(_) => Some("array".to_string()),
        TypeExpr::Ptr(_) | TypeExpr::Buffer(_) => None,
        TypeExpr::Named(name) => Some(name.as_str().to_string()),
        TypeExpr::Nullable(inner) => {
            let inner = eval_native_type_expr_spec(inner)?;
            Some(format!("?{}", inner))
        }
        TypeExpr::Union(members) => eval_native_type_expr_member_specs(members, "|"),
        TypeExpr::Intersection(members) => eval_native_type_expr_member_specs(members, "&"),
    }
}

/// Formats a compound parsed type expression with the requested separator.
pub(super) fn eval_native_type_expr_member_specs(members: &[TypeExpr], separator: &str) -> Option<String> {
    members
        .iter()
        .map(eval_native_type_expr_spec)
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join(separator))
}

/// Formats one checked PHP type for eval native metadata registration.
pub(super) fn eval_native_php_type_spec(php_type: &PhpType, allow_return_atoms: bool) -> Option<String> {
    match php_type {
        PhpType::Int => Some("int".to_string()),
        PhpType::Float => Some("float".to_string()),
        PhpType::Str => Some("string".to_string()),
        PhpType::Bool => Some("bool".to_string()),
        PhpType::False => Some("false".to_string()),
        PhpType::Void if allow_return_atoms => Some("void".to_string()),
        PhpType::Void => Some("null".to_string()),
        PhpType::Never if allow_return_atoms => Some("never".to_string()),
        PhpType::Never => None,
        PhpType::Iterable => Some("iterable".to_string()),
        PhpType::Mixed => Some("mixed".to_string()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some("array".to_string()),
        PhpType::Callable => Some("callable".to_string()),
        PhpType::Object(name) if name.is_empty() => Some("object".to_string()),
        PhpType::Object(name) => Some(name.clone()),
        PhpType::Union(members) => eval_native_php_type_member_specs(members),
        PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => None,
    }
}

/// Formats union members from checked PHP types for eval native metadata registration.
pub(super) fn eval_native_php_type_member_specs(members: &[PhpType]) -> Option<String> {
    members
        .iter()
        .map(|member| eval_native_php_type_spec(member, false))
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join("|"))
}

/// The explicit PHP signature shape a generated bridge registers alongside its parameter slots.
///
/// Every field is read from the AST-level `FunctionSig`, where a declared default is present as
/// an EXPRESSION whether or not its value can be represented in the eval default ABI. That is the
/// whole reason this record exists: an enum-case or deeply nested constant default registers no
/// default at all, so nothing downstream may recover the PHP arity by looking for one.
pub(super) struct EvalNativeSignatureShape {
    /// PHP-visible non-variadic parameters, the leading run of registered slots.
    pub(super) visible_regular_param_count: usize,
    /// Mandatory PHP-visible parameters, from the source declaration.
    pub(super) required_param_count: usize,
    /// Whether the registered variadic slot is one the PHP source declared.
    pub(super) source_variadic: bool,
    /// Whether the hidden collector's first element carries the actual PHP argument count.
    pub(super) collector_carries_count: bool,
}

/// Shape-flags bit meaning "the registered variadic slot is source-declared".
///
/// Mirrors `elephc_magician::context::NATIVE_SHAPE_FLAG_SOURCE_VARIADIC`. Magician is a
/// dev-dependency of the compiler, not a dependency, so the numbering is spelled on both sides
/// and the two constants must be changed together.
const NATIVE_SHAPE_FLAG_SOURCE_VARIADIC: i64 = 1 << 0;

/// Shape-flags bit meaning "the hidden collector's first element is the actual argument count".
///
/// Mirrors `elephc_magician::context::NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT`.
const NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT: i64 = 1 << 1;

impl EvalNativeSignatureShape {
    /// Returns the packed flags word this shape registers.
    pub(super) fn flags(&self) -> i64 {
        let mut flags = 0;
        if self.source_variadic {
            flags |= NATIVE_SHAPE_FLAG_SOURCE_VARIADIC;
        }
        if self.collector_carries_count {
            flags |= NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT;
        }
        flags
    }
}

/// Derives the registered PHP signature shape from one bridge-compatible signature.
///
/// `regular_param_count` already excludes both the variadic slot and the hidden
/// `__elephc_func_argc` parameter, so the required count is meaningful over exactly that prefix.
pub(super) fn eval_native_signature_shape(sig: &FunctionSig) -> EvalNativeSignatureShape {
    let visible_regular_param_count = crate::types::call_args::regular_param_count(sig);
    let required_param_count = (0..visible_regular_param_count)
        .rfind(|index| sig.defaults.get(*index).is_none_or(Option::is_none))
        .map_or(0, |index| index + 1);
    EvalNativeSignatureShape {
        visible_regular_param_count,
        required_param_count,
        source_variadic: sig
            .variadic
            .as_deref()
            .is_some_and(|variadic| variadic != crate::func_args::HIDDEN_ARGS_PARAM),
        collector_carries_count: crate::func_args::sig_collects_optional_arg_count(sig),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a signature whose hidden slots mirror what `crate::func_args` appends.
    ///
    /// `defaults` carries an EXPRESSION per slot, which is what the registration reads. Whether a
    /// given default has an eval representation is a separate question the shape never asks.
    fn sig(params: Vec<&str>, defaults: Vec<bool>, variadic: Option<&str>) -> FunctionSig {
        let count = params.len();
        FunctionSig {
            params: params
                .into_iter()
                .map(|name| (name.to_string(), PhpType::Mixed))
                .collect(),
            param_type_exprs: vec![None; count],
            param_attributes: vec![Vec::new(); count],
            defaults: defaults
                .into_iter()
                .map(|has_default| {
                    has_default.then(|| {
                        crate::parser::ast::Expr::new(
                            crate::parser::ast::ExprKind::IntLiteral(0),
                            crate::span::Span::dummy(),
                        )
                    })
                })
                .collect(),
            return_type: PhpType::Mixed,
            declared_return: false,
            by_ref_return: false,
            ref_params: vec![false; count],
            declared_params: vec![false; count],
            variadic: variadic.map(str::to_string),
            deprecation: None,
        }
    }

    /// A plain signature registers its source arity and no hidden-slot flags.
    #[test]
    fn a_plain_signature_registers_its_source_arity() {
        let shape = eval_native_signature_shape(&sig(vec!["a", "b"], vec![false, true], None));
        assert_eq!(shape.visible_regular_param_count, 2);
        assert_eq!(shape.required_param_count, 1);
        assert!(!shape.source_variadic);
        assert!(!shape.collector_carries_count);
        assert_eq!(shape.flags(), 0);
    }

    /// A source variadic behind a hidden count parameter is reported as source-declared.
    ///
    /// The required count must stop at the visible regulars: the hidden count parameter carries a
    /// synthesized `0` default of its own, and the variadic carries none, so counting over the
    /// physical list would answer for a signature the PHP source never wrote.
    #[test]
    fn a_source_variadic_behind_a_hidden_count_is_flagged_as_source_declared() {
        let signature = sig(
            vec!["a", "b", crate::func_args::HIDDEN_ARGC_PARAM, "rest"],
            vec![false, true, true, false],
            Some("rest"),
        );
        let shape = eval_native_signature_shape(&signature);
        assert_eq!(shape.visible_regular_param_count, 2);
        assert_eq!(shape.required_param_count, 1);
        assert!(shape.source_variadic);
        assert!(!shape.collector_carries_count);
        assert_eq!(shape.flags(), NATIVE_SHAPE_FLAG_SOURCE_VARIADIC);
        // The registration walks exactly the source slots: the hidden count is not among them.
        assert_eq!(source_declared_param_indexes(&signature), vec![0, 1, 3]);
    }

    /// A hidden collector next to an optional regular carries the actual argument count.
    #[test]
    fn a_hidden_collector_next_to_an_optional_regular_carries_the_count() {
        let signature = sig(
            vec!["a", "b", crate::func_args::HIDDEN_ARGS_PARAM],
            vec![false, true, false],
            Some(crate::func_args::HIDDEN_ARGS_PARAM),
        );
        let shape = eval_native_signature_shape(&signature);
        assert_eq!(shape.visible_regular_param_count, 2);
        assert_eq!(shape.required_param_count, 1);
        assert!(!shape.source_variadic);
        assert!(shape.collector_carries_count);
        assert_eq!(shape.flags(), NATIVE_SHAPE_FLAG_COLLECTOR_CARRIES_COUNT);
        // The collector is compiler-internal, so a free function never registers it at all.
        assert_eq!(source_declared_param_indexes(&signature), vec![0, 1]);
    }

    /// A hidden collector with no optional regular needs no count prefix.
    #[test]
    fn a_hidden_collector_with_only_mandatory_regulars_needs_no_count() {
        let shape = eval_native_signature_shape(&sig(
            vec!["a", crate::func_args::HIDDEN_ARGS_PARAM],
            vec![false, false],
            Some(crate::func_args::HIDDEN_ARGS_PARAM),
        ));
        assert_eq!(shape.visible_regular_param_count, 1);
        assert_eq!(shape.required_param_count, 1);
        assert!(!shape.collector_carries_count);
        assert_eq!(shape.flags(), 0);
    }
}
