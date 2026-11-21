//! Purpose:
//! Evaluates operands with logical source snapshots at their actual source-order positions.
//!
//! Called from:
//! - Mixed-aware assignments and scalar expression consumers.
//!
//! Key details:
//! - Value owners remain in the existing runtime owner frames through effects and cleanup.
//! - Source metadata is returned with the result, never indexed by a runtime value handle.

use super::*;
use elephc_generics::Type;
use crate::eval_ir::generic_type;
use crate::runtime_hooks::owner_frames::with_runtime_owners;

/// Evaluates a source once, keeping declared returns independent from their concrete occupants.
pub(super) fn evaluate(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    if mixed_class_sources::handles(expr) {return mixed_class_sources::evaluate(expr,context,scope,values);}
    if nullsafe_chains::receiver(expr).is_some() {
        return nullsafe_chains::evaluate_with_source(expr,false,false,context,scope,values)?
            .map_or_else(||values.null().map(|value|(value,Some(Type::atom("null")))),Ok);
    }
    match expr {
        EvalExpr::Array(elements) => {
            let (value, source) = array_literals::evaluate_argument_array(elements, context, scope, values)?;
            Ok((value, Some(generic_type(&source.ty))))
        }
        EvalExpr::Binary { op, left, right } if !matches!(op, EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr) =>
            binary(*op, left, right, context, scope, values),
        EvalExpr::Call { name, args } if forwarded(name, None, context) =>
            eval_owned_builtin_call_with_source(name.trim_start_matches('\\').to_ascii_lowercase().as_str(), args, false, context, scope, values),
        EvalExpr::NamespacedCall { name, fallback_name, args } if forwarded(name, Some(fallback_name), context) =>
            eval_owned_builtin_call_with_source(fallback_name.trim_start_matches('\\').to_ascii_lowercase().as_str(), args, false, context, scope, values),
        EvalExpr::DynamicCall{callee,args}=>dynamic_call(callee,args,context,scope,values),
        EvalExpr::Unary { op, expr } => unary(*op, expr, context, scope, values),
        EvalExpr::Ternary { condition, then_branch: Some(then_branch), else_branch } => {
            let selected = if eval_condition(condition, context, scope, values)? { then_branch } else { else_branch };
            evaluate(selected, context, scope, values)
        }
        EvalExpr::Ternary { condition,then_branch:None,else_branch }=>{
            let (value,source)=evaluate(condition,context,scope,values)?;
            let checked=mixed_source_proofs::require_condition(&whole_source(source.as_ref()),context,values)
                .and_then(|()|eval_boolean_condition(value,context,values));
            if matches!(checked,Ok(true)) {return Ok((value,source));}
            let released=eval_release_value(context,values,value);
            checked?;released?;
            evaluate(else_branch,context,scope,values)
        }
        EvalExpr::Assign { name, .. } => {
            let value = eval_owned_expr(expr, context, scope, values)?;
            let source = source_type(&EvalExpr::LoadVar(name.clone()), context, scope);
            Ok((value, source))
        }
        _ => {
            let source = source_type(expr, context, scope);
            let value = eval_owned_expr(expr, context, scope, values)?;
            Ok((value, source))
        }
    }
}

/// Captures one callee before argument effects and returns its declaration-backed result evidence.
pub(super) fn dynamic_call(callee: &EvalExpr,args: &[EvalCallArg],context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    with_sources(&[callee],context,scope,values,|operands,sources,context,scope,values|{
        let source=whole_source(sources[0].as_ref());
        mixed_source_proofs::require_source(&source,"callable invocation",context,values,
            mixed_callable_sources::supports)?;
        eval_dynamic_call_with_callback_source(operands[0],args,context,scope,values)
    })
}

/// Takes a producer snapshot without inferring a proof from its evaluated runtime tag.
fn source_type(expr: &EvalExpr, context: &ElephcEvalContext, scope: &ElephcEvalScope) -> Option<Type> {
    local_binding_origins::source_view(expr, context, scope).as_ref().map(generic_type)
        .or_else(|| matches!(expr, EvalExpr::Array(_)).then(|| Type::atom("array")))
}

/// Roots each detached operand before evaluating the next source and preserves failure cleanup.
pub(super) fn with_sources<V: RuntimeValueOps>(args: &[&EvalExpr], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &[Option<Type>], &mut ElephcEvalContext,
        &mut ElephcEvalScope, &mut V) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus>,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    let mut owners = Vec::with_capacity(args.len()); let mut sources = Vec::with_capacity(args.len());
    let prepared = (|| {
        for arg in args {
            let (value, source) = with_runtime_owners(&owners, || evaluate(arg, context, scope, values))?;
            owners.push(value); sources.push(source);
        }
        Ok(())
    })();
    let result = prepared.and_then(|()| with_runtime_owners(&owners, ||
        consume(&owners, &sources, context, scope, values)));
    match result {
        Ok((value, source)) => finish_eval_argument_values(Ok(value), owners, context, values)
            .map(|value| (value, source)),
        Err(status) => finish_eval_argument_values(Err(status), owners, context, values)
            .map(|_| unreachable!("failed source evaluation cannot produce a value")),
    }
}

/// Validates complete source alternatives after both ordinary operands have run.
pub(super) fn binary(op: EvalBinOp, left: &EvalExpr, right: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    with_sources(&[left, right], context, scope, values, |operands, sources, context, _, values| {
        let left = whole_source(sources[0].as_ref()); let right = whole_source(sources[1].as_ref());
        mixed_source_proofs::require_binary(op, &left, &right, context, values)?;
        if op==EvalBinOp::LogicalXor {
            mixed_source_proofs::require_condition(&left,context,values)?;
            mixed_source_proofs::require_condition(&right,context,values)?;
        }
        let value = eval_binary_result(op, operands[0], operands[1], context, values)?;
        Ok((value, Some(mixed_result_sources::binary(op, &left, &right, context))))
    })
}

/// Applies unary source proofs before delegating value semantics to the existing operation adapter.
pub(super) fn unary(op: EvalUnaryOp, expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    with_sources(&[expr], context, scope, values, |operands, sources, context, _, values| {
        let operand = whole_source(sources[0].as_ref());
        if op==EvalUnaryOp::LogicalNot {mixed_source_proofs::require_condition(&operand,context,values)?;}
        else {
            mixed_source_proofs::require_source(&operand, "unary operations", context, values,
                |ty, context| mixed_source_proofs::unary_source_supported(op, ty, context))?;
        }
        let value = eval_unary_value(op, operands[0], context, values)?;
        let source=mixed_result_sources::unary(op,&operand,context);
        Ok((value, Some(source)))
    })
}

/// Normalizes only the consumed whole-kind proof, preserving optional producer metadata in transit.
fn whole_source(source: Option<&Type>) -> Type {
    source.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(|| Type::atom("mixed"))
}

/// Identifies forwarding builtins after namespace fallback and user declaration selection.
fn forwarded(name: &str, fallback: Option<&str>, context: &ElephcEvalContext) -> bool {
    local_binding_origins::selected_call_builtin(name, fallback, context)
        .is_some_and(|contract| matches!(contract.name, "call_user_func" | "call_user_func_array"))
}
