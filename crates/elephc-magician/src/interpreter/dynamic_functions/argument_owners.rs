//! Purpose:
//! Owns temporary property receivers while eval call arguments borrow their reference targets.
//!
//! Called from:
//! - The shared eval function, method and constructor call-argument boundary.
//!
//! Key details:
//! - Reference metadata remains borrowed and survives until argument writeback completes.
//! - Failed argument evaluation retires both completed values and receiver owners.

use super::*;

/// Acquires each argument before later argument side effects can replace its source storage.
pub(in crate::interpreter) fn eval_owned_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(Vec<EvaluatedCallArg>, Vec<RuntimeCellHandle>), EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut receiver_owners = Vec::new();
    let mut saw_named = false;

    let evaluated = (|| {
        for arg in args {
            if arg.is_spread() {
                if saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let spread = eval_expr(arg.value(), context, caller_scope, values)?;
                let unpacked = (|| {
                    if !values.is_array_like(spread)? {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    let first_unpacked = evaluated_args.len();
                    append_unpacked_call_arg_values(
                        spread, &mut evaluated_args, &mut saw_named, context, values,
                    )?;
                    for argument in &mut evaluated_args[first_unpacked..] {
                        if argument.value.is_borrowed() {
                            argument.value = values.retain(argument.value)?;
                        }
                    }
                    Ok(())
                })();
                let released = release_expr_result(spread, context, values);
                unpacked.and(released)?;
                continue;
            }

            if let Some(name) = arg.name() {
                saw_named = true;
                let (value, mut ref_target) =
                    eval_call_arg_value(arg.value(), context, caller_scope, values)?;
                capture_argument_receiver_owners(ref_target.as_mut(), &mut receiver_owners);
                let value = if value.is_borrowed() {
                    values.retain(value)?
                } else {
                    value
                };
                evaluated_args.push(EvaluatedCallArg {
                    name: Some(name.to_string()),
                    value,
                    ref_target,
                });
                continue;
            }

            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let (value, mut ref_target) = eval_call_arg_value(arg.value(), context, caller_scope, values)?;
            capture_argument_receiver_owners(ref_target.as_mut(), &mut receiver_owners);
            let value = if value.is_borrowed() {
                values.retain(value)?
            } else {
                value
            };
            evaluated_args.push(EvaluatedCallArg {
                name: None,
                value,
                ref_target,
            });
        }
        Ok(())
    })();
    if let Err(status) = evaluated {
        for argument in evaluated_args {
            let _ = release_expr_result(argument.value, context, values);
        }
        for owner in receiver_owners.into_iter().rev() {
            let _ = release_expr_result(owner, context, values);
        }
        return Err(status);
    }
    Ok((evaluated_args, receiver_owners))
}

/// Moves independently owned property receivers out of borrowed call reference metadata.
fn capture_argument_receiver_owners(
    target: Option<&mut EvalReferenceTarget>, owners: &mut Vec<RuntimeCellHandle>,
) {
    match target {
        Some(EvalReferenceTarget::ObjectProperty { object, .. }) if !object.is_borrowed() => {
            owners.push(*object);
            *object = object.borrowed();
        }
        Some(EvalReferenceTarget::NestedArrayElement { array_target, .. }) => {
            capture_argument_receiver_owners(Some(array_target.as_mut()), owners);
        }
        _ => {}
    }
}
