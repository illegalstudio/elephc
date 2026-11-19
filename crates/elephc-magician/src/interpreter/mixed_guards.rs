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

/// Records the observed branch fact while retaining unrelated producer obligations.
pub(super) fn record(condition: &EvalExpr, truthy: bool,
    context: &ElephcEvalContext, scope: &mut ElephcEvalScope,
) {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return; }
    if let EvalExpr::Unary { op: EvalUnaryOp::LogicalNot, expr } = condition {
        record(expr, !truthy, context, scope); return;
    }
    let EvalExpr::Call { name, args } = condition else { return; };
    if args.len() != 1 || context.function(name).is_some() { return; }
    let Some(contract) = elephc_builtin_contract::lookup(name.trim_start_matches('\\')) else { return; };
    let EvalExpr::LoadVar(name) = args[0].value() else { return; };
    let target = match contract.name {
        "is_int" | "is_integer" | "is_long" => Type::atom("int"),
        "is_float" | "is_double" | "is_real" => Type::atom("float"),
        "is_bool" => Type::atom("bool"), "is_string" => Type::atom("string"),
        "is_array" => Type::atom("array"), "is_object" => Type::atom("object"),
        "is_null" => Type::atom("null"), "is_callable" => Type::atom("callable"),
        "is_iterable" => Type::atom("iterable"),
        "is_resource" => Type::atom(elephc_semantics::native_local_contract::RESOURCE_TYPE_SPEC),
        _ => return,
    };
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
