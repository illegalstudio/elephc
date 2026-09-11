//! Purpose:
//! Validates statically impossible mbstring arguments while preserving runtime PHP coercion.
//!
//! Called from:
//! - Individual mbstring builtin semantic descriptors after shared argument planning.
//!
//! Key details:
//! - PHP parameter metadata remains authoritative in the neutral builtin contract.
//! - Nullable scalar unions preserve null across dynamic function boundaries.
//! - Dynamic values reach the shared planner with their original types and caller strictness.

use elephc_builtin_contract::TypeSpec;
use crate::{builtins::spec::BuiltinCheckCtx, errors::CompileError, types::PhpType};

/// Validates capture inputs without reading the write-only output argument.
pub(crate) fn capture_check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let contract = elephc_builtin_contract::lookup(cx.name).expect("registered mbstring contract");
    let input_count = contract.params.iter().position(|parameter| parameter.by_ref).unwrap_or(contract.params.len());
    let mut inputs = BuiltinCheckCtx {
        checker: cx.checker, name: cx.name, args: &cx.args[..cx.args.len().min(input_count)],
        argument_plan: cx.argument_plan, span: cx.span, env: cx.env,
    };
    check(&mut inputs)
}

/// Identifies the capture output at its source position using the shared contract's parameter name.
pub(crate) fn is_capture_output_argument(name: &str, arg: &crate::parser::ast::Expr, position: usize) -> bool {
    use elephc_builtin_contract::RuntimeBuiltinId;
    let Some(definition) = super::registry::lookup(name) else { return false; };
    if !matches!(definition.spec.runtime_builtin_id(), Some(RuntimeBuiltinId::MbEreg | RuntimeBuiltinId::MbEregi | RuntimeBuiltinId::MbParseStr)) {
        return false;
    }
    let contract = elephc_builtin_contract::lookup_id(definition.spec.runtime_builtin_id().unwrap().builtin_id())
        .expect("registered mbstring contract");
    let Some(output) = contract.params.iter().position(|parameter| parameter.by_ref) else { return false; };
    match &arg.kind {
        crate::parser::ast::ExprKind::NamedArg { name, .. } => contract.params[output].name == name,
        _ => position == output,
    }
}

/// Checks planned arguments against their shared parameter contracts and returns the declared type.
pub(crate) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_with_preinferred_argument(cx, None)
}

/// Checks planned arguments while reusing one type already inferred by a lazy checker hook.
pub(crate) fn check_with_known_argument(
    cx: &mut BuiltinCheckCtx,
    argument_index: usize,
    argument_type: PhpType,
) -> Result<PhpType, CompileError> {
    check_with_preinferred_argument(cx, Some((argument_index, argument_type)))
}

/// Applies the shared mbstring contract with an optional preinferred argument type.
fn check_with_preinferred_argument(
    cx: &mut BuiltinCheckCtx,
    known_argument: Option<(usize, PhpType)>,
) -> Result<PhpType, CompileError> {
    let contract = elephc_builtin_contract::lookup(cx.name).expect("registered mbstring contract");
    let mut dynamic_position = false;
    for (index, argument) in cx.args.iter().enumerate() {
        let ty = if known_argument.as_ref().is_some_and(|(known_index, _)| *known_index == index) {
            known_argument
                .as_ref()
                .expect("known argument index must retain its type")
                .1
                .clone()
        } else {
            cx.checker.infer_type(argument, cx.env)?
        };
        dynamic_position |= matches!(argument.kind, crate::parser::ast::ExprKind::Spread(_));
        if dynamic_position { continue; }
        let Some(parameter) = contract.params.get(index) else { continue; };
        if !accepts_in_mode(&ty, &parameter.ty, cx.checker.strict_types) {
            let expected = description(&parameter.ty);
            return Err(CompileError::new(argument.span,
                &format!("{}() {} argument must be {}", contract.name, parameter.name, expected)));
        }
    }
    Ok(super::convert::type_spec_to_php(&contract.returns))
}

/// Rejects statically impossible parameter shapes while leaving actual scalar conversions to the shared engine.
fn accepts_in_mode(ty: &PhpType, expected: &TypeSpec, strict: bool) -> bool {
    match ty {
        PhpType::Never | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(|ty| accepts_in_mode(ty, expected, strict)),
        _ => match expected {
            TypeSpec::Mixed => true,
            TypeSpec::Callable => matches!(ty, PhpType::Callable | PhpType::Str | PhpType::Object(_)
                | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable),
            TypeSpec::Str => *ty == PhpType::Str || !strict && matches!(ty,
                PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Void
                | PhpType::Object(_) | PhpType::Callable),
            TypeSpec::Array => matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable),
            TypeSpec::Int => *ty == PhpType::Int || !strict && scalar_coercion_source(ty),
            TypeSpec::Bool => matches!(ty, PhpType::Bool | PhpType::False)
                || !strict && scalar_coercion_source(ty),
            TypeSpec::False => *ty == PhpType::False,
            TypeSpec::Null => *ty == PhpType::Void,
            TypeSpec::Union(members) => members.iter().any(|expected| accepts_in_mode(ty, expected, strict)),
            TypeSpec::Nullable(inner) => *ty == PhpType::Void || accepts_in_mode(ty, inner, strict),
            _ => false,
        },
    }
}

/// Identifies weak scalar inputs whose precise numeric/null conversion is decided by the runtime planner.
fn scalar_coercion_source(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str | PhpType::Void)
}

/// Describes scalar and nullable parameter types in compiler diagnostics.
fn description(expected: &TypeSpec) -> String {
    match expected {
        TypeSpec::Mixed => "mixed".into(),
        TypeSpec::Callable => "callable".into(),
        TypeSpec::Array => "array".into(),
        TypeSpec::Int => "int".into(),
        TypeSpec::Bool => "bool".into(),
        TypeSpec::False => "false".into(),
        TypeSpec::Null => "null".into(),
        TypeSpec::Union(members) => members.iter().map(description).collect::<Vec<_>>().join(" or "),
        TypeSpec::Nullable(inner) => format!("{} or null", description(inner)),
        _ => "string".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Permits every current capture-output shape without weakening ordinary scalar parameters.
    #[test]
    fn mixed_capture_parameter_accepts_concrete_values_in_both_modes() {
        for strict in [false, true] {
            for actual in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::False,
                PhpType::Void, PhpType::Str, PhpType::Array(Box::new(PhpType::Str)),
                PhpType::Object("CaptureOwner".into()), PhpType::Callable,
                PhpType::Union(vec![PhpType::Int, PhpType::Void])] {
                assert!(accepts_in_mode(&actual, &TypeSpec::Mixed, strict), "{actual:?}, strict={strict}");
            }
            assert!(!accepts_in_mode(&PhpType::Int, &TypeSpec::Array, strict));
            assert!(!accepts_in_mode(&PhpType::Array(Box::new(PhpType::Str)), &TypeSpec::Str, strict));
        }
        assert!(!accepts_in_mode(&PhpType::Int, &TypeSpec::Str, true));
    }
}
