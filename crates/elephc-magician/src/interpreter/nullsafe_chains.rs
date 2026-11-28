//! Purpose:
//! Evaluates complete nullsafe postfix chains without confusing short circuits with null values.
//!
//! Called from:
//! - Ordinary expressions, coalescing and the existing empty nullsafe adapter.
//!
//! Key details:
//! - None propagates an upstream short circuit without evaluating later selectors or arguments.
//! - Each intermediate receiver keeps its owner until its successor has an owner.

use super::*;
use elephc_generics::Type;
mod source_types;
pub(super) use source_types::{offset_key as validate_offset_key_source, property as property_source};

/// Finds only postfix receivers, keeping keys and arguments outside the chain's structure.
pub(super) fn receiver(expr: &EvalExpr) -> Option<(&EvalExpr, bool)> {
    match expr {
        EvalExpr::NullsafePropertyGet { object, .. } | EvalExpr::NullsafeDynamicPropertyGet { object, .. }
        | EvalExpr::NullsafeMethodCall { object, .. } | EvalExpr::NullsafeDynamicMethodCall { object, .. } => Some((object, true)),
        EvalExpr::PropertyGet { object, .. } | EvalExpr::DynamicPropertyGet { object, .. }
        | EvalExpr::MethodCall { object, .. } | EvalExpr::DynamicMethodCall { object, .. } => Some((object, false)),
        EvalExpr::ArrayGet { array, .. } => Some((array, false)),
        EvalExpr::DynamicCall { callee, .. } => Some((callee, false)),
        _ => None,
    }
}

/// Detects nullsafe hops without evaluating any part of the operand.
pub(super) fn has_nullsafe(expr: &EvalExpr) -> bool {
    receiver(expr).is_some_and(|(object, nullsafe)| nullsafe || has_nullsafe(object))
}

/// Produces one owned receiver or propagates a prior nullsafe short circuit unchanged.
pub(super) fn evaluate_receiver(
    expr: &EvalExpr, quiet: bool, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    evaluate_with_source(expr,quiet,false,context,scope,values).map(|result|result.map(|result|result.0))
}

/// Produces an owned value and optional declaration, retaining None exclusively for skipped hops.
pub(super) fn evaluate_with_source(
    expr: &EvalExpr, quiet: bool, discarded: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<Option<(RuntimeCellHandle,Option<Type>)>,EvalStatus> {
    let mixed=context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed);
    let Some((receiver_expr,nullsafe))=receiver(expr) else {
        if quiet && matches!(expr,EvalExpr::LoadVar(_)) {
            let source=local_binding_origins::source_view(expr,context,scope).as_ref().map(crate::eval_ir::generic_type);
            return probe_reads::eval_owned(expr,context,scope,values).map(|value|Some((value,source)));
        }
        return if mixed {mixed_eval::evaluate(expr,context,scope,values).map(Some)}
            else {eval_owned_expr(expr,context,scope,values).map(|value|Some((value,None)))};
    };
    let Some((object,source))=evaluate_with_source(receiver_expr,quiet,false,context,scope,values)? else {return Ok(None);};
    let result=(|| {
        if nullsafe && values.is_null(object)? {
            source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)?;
            return Ok(None);
        }
        let result=match expr {
            EvalExpr::PropertyGet {property,..}|EvalExpr::NullsafePropertyGet {property,..}=> {
                source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)?;
                let source=mixed_guards::projection_source(expr,context,scope).as_ref().map(crate::eval_ir::generic_type)
                    .or(source_types::property(object,property,context,values)?);
                property_value(object,property,quiet,context,values).map(|value|(value,source))
            }
            EvalExpr::DynamicPropertyGet {property,..}|EvalExpr::NullsafeDynamicPropertyGet {property,..}=> {
                let property=eval_dynamic_member_name(property,context,scope,values)?;
                source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)?;
                let source=mixed_guards::projection_source(expr,context,scope).as_ref().map(crate::eval_ir::generic_type)
                    .or(source_types::property(object,&property,context,values)?);
                property_value(object,&property,quiet,context,values).map(|value|(value,source))
            }
            EvalExpr::ArrayGet {index,..}=> {
                let (index,key_source)=if mixed {mixed_eval::evaluate(index,context,scope,values)?}
                    else {(eval_owned_expr(index,context,scope,values)?,None)};
                let result=source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)
                    .and_then(|()|source_types::offset_key(source.as_ref(),key_source.as_ref(),context,values)).and_then(|()|
                    if quiet {eval_array_get_quiet_result(object,index,context,values)}
                    else {eval_array_get_result(object,index,context,values)});
                finish_eval_argument_values(result,[index],context,values)
                    .map(|value|(value,mixed_guards::projection_source(expr,context,scope).as_ref().map(crate::eval_ir::generic_type)
                        .or_else(||source.as_ref().and_then(local_binding_origins::declared_offset_result))))
            }
            EvalExpr::MethodCall {method,args,..}|EvalExpr::NullsafeMethodCall {method,args,..}=> {
                source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)?;
                method_value(object,method,args,discarded,mixed,context,scope,values)
            }
            EvalExpr::DynamicMethodCall {method,args,..}|EvalExpr::NullsafeDynamicMethodCall {method,args,..}=> {
                let method=eval_dynamic_member_name(method,context,scope,values)?;
                source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)?;
                method_value(object,&method,args,discarded,mixed,context,scope,values)
            }
            EvalExpr::DynamicCall {args,..}=> {
                source_types::receiver(expr,source.as_ref(),nullsafe,quiet,context,values)?;
                eval_dynamic_call_with_callback_source(object,args,context,scope,values)
            }
            _=>Err(EvalStatus::RuntimeFatal),
        };
        result.and_then(|(value,source)| {
            if value.is_borrowed() {values.retain(value)} else {Ok(value)}
                .map(|value|Some((value,source)))
        })
    })();
    let cleanup=release_expr_result(object,context,values);
    match (result,cleanup) {
        (Ok(result),Ok(()))=>Ok(result),
        (Err(status),_)=>Err(status),
        (Ok(result),Err(status))=> {
            if let Some((value,_))=result {let _=release_expr_result(value,context,values);}
            Err(status)
        }
    }
}

/// Keeps permissive value-only dispatch intact while selected Mixed methods use shared result sources.
fn method_value(object: RuntimeCellHandle, method: &str, args: &[EvalCallArg], discarded: bool,
    mixed: bool, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    if mixed {return mixed_method_results::selected_object(object,method,args,discarded,context,scope,values);}
    with_eval_object_method_arguments(args,object,method,context,scope,values,|args,context,_,values|
        eval_method_call_result_with_evaluated_args(object,method,args,context,values)).map(|value|(value,None))
}

/// Reads one property under its ordinary or explicit quiet receiver contract.
fn property_value(object: RuntimeCellHandle, property: &str, quiet: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if quiet && values.is_null(object)? {return values.null();}
    if quiet && (context.semantic_profile().contains(elephc_semantics::StrictFeature::Nullability)
        || context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed))
        && !initialization::coalesce_uses_getter(object, property, context, values)
        && !eval_property_isset_result(object, property, context, values)? {
        return values.null();
    }
    eval_property_get_result(object, property, context, values)
}
