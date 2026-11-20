//! Purpose:
//! Records predicate-derived logical views separately from broad producer declarations.
//!
//! Called from:
//! - Successfully evaluated branch conditions under the Mixed guarantee.
//!
//! Key details:
//! - Scope generations and canonical reference write observations retire stale facts.
//! - Predicate recognition uses the builtin contract and never executes PHP for a proof.

use super::*;
use elephc_generics::Type;
use crate::eval_ir::{generic_type, parameter_type};
mod places;
pub(super) use places::{prepare, record as record_projection, source as projection_source};

/// Records the observed branch fact while retaining unrelated producer obligations.
pub(super) fn record(condition: &EvalExpr, truthy: bool,
    context: &ElephcEvalContext, scope: &mut ElephcEvalScope,
) {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return; }
    if let EvalExpr::Unary { op: EvalUnaryOp::LogicalNot, expr } = condition {
        record(expr, !truthy, context, scope); return;
    }
    let Some((argument, target)) = predicate(condition, context) else { return; };
    let EvalExpr::LoadVar(name) = argument else { return; };
    let source = if truthy { Some(target) } else {
        local_binding_origins::source_view(&EvalExpr::LoadVar(name.clone()), context, scope)
            .as_ref().and_then(|source| exclude(generic_type(source), &target))
    };
    if let Some(source) = source {
        scope.record_local_guard_source_view(name.clone(), parameter_type(&source), context.source_view_epoch());
    }
}

/// Removes only a represented finite alternative; an unknown mixed remainder stays unknown.
fn exclude(source: Type, target: &Type) -> Option<Type> {
    match source {
        Type::Union(members) => {
            let mut retained = members.into_iter().filter(|member| member != target).collect::<Vec<_>>();
            match retained.len() { 0 => None, 1 => retained.pop(), _ => Some(Type::Union(retained)) }
        }
        Type::Nullable(inner) => exclude(Type::Union(vec![*inner, Type::atom("null")]), target),
        _ => None,
    }
}

/// Selects only reached catalog predicates, respecting namespace fallback and user shadowing.
fn predicate<'a>(condition: &'a EvalExpr, context: &ElephcEvalContext) -> Option<(&'a EvalExpr, Type)> {
    let (name, fallback, args) = match condition {
        EvalExpr::Call { name, args } => (name, None, args),
        EvalExpr::NamespacedCall { name, fallback_name, args } => (name, Some(fallback_name.as_str()), args),
        _ => return None,
    };
    if args.len() != 1 { return None; }
    let Some(contract) = local_binding_origins::selected_call_builtin(name, fallback, context) else { return None; };
    let target = match contract.name {
        "is_int" | "is_integer" | "is_long" => Type::atom("int"),
        "is_float" | "is_double" | "is_real" => Type::atom("float"),
        "is_bool" => Type::atom("bool"), "is_string" => Type::atom("string"),
        "is_array" => Type::atom("array"), "is_object" => Type::atom("object"),
        "is_null" => Type::atom("null"), "is_callable" => Type::atom("callable"),
        "is_iterable" => Type::atom("iterable"),
        "is_resource" => Type::atom(elephc_semantics::native_local_contract::RESOURCE_TYPE_SPEC),
        _ => return None,
    };
    Some((args[0].value(), target))
}
