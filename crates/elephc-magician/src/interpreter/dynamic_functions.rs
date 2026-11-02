//! Purpose:
//! Coordinates call-argument evaluation for user-declared and native functions.
//! Binding and execution paths live in focused child modules.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and dynamic callable dispatch helpers.
//!
//! Key details:
//! - PHP source evaluation order is preserved before argument binding.
//! - Static locals are persisted through `ElephcEvalContext` after function execution.

mod closure_execution;
mod function_binding;
mod function_staging;
mod method_binding;
mod native_execution;
pub(in crate::interpreter) mod builtin_arguments;

use super::*;

pub(in crate::interpreter) use closure_execution::*;
pub(in crate::interpreter) use function_binding::*;
use function_staging::stage_native_function_invoker_args;
pub(in crate::interpreter) use method_binding::*;
pub(in crate::interpreter) use native_execution::*;
pub(crate) use native_execution::current_native_call_context;

/// Evaluates an eval-declared user function with PHP-style argument binding.
pub(in crate::interpreter) fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, caller_scope, values, |arguments, context, _, values| {
        eval_dynamic_function_with_evaluated_args(function, arguments, context, values)
    })
}

/// Acquires each argument before later argument side effects can replace its source storage.
pub(in crate::interpreter) fn eval_owned_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut saw_named = false;

    let evaluated = (|| {
        for arg in args {
            if arg.is_spread() {
                if saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let spread = eval_expr(arg.value(), context, caller_scope, values)?;
                // A Traversable operand unpacks as the array its iteration yields. The merge with
                // origin/main dropped this from the user-call path (the builtin path below kept
                // it), and Doctrine's `new Transformations(...Inflectible::getPlural())` received
                // no arguments: `Str::plural()` hit "foreach() argument must be of type
                // array|object, null given" and Laravel's `/strings` page answered 500.
                let traversed = match values.type_tag(spread) {
                    Ok(tag) if tag == EVAL_TAG_OBJECT => {
                        match super::array_literals::eval_spread_traversable_array(
                            spread, context, values,
                        ) {
                            Ok(array) => Some(array),
                            Err(status) => {
                                let _ = release_expr_result(spread, context, values);
                                return Err(status);
                            }
                        }
                    }
                    Ok(_) => None,
                    Err(status) => {
                        let _ = release_expr_result(spread, context, values);
                        return Err(status);
                    }
                };
                let source = traversed.unwrap_or(spread);
                let unpacked = (|| {
                    if !values.is_array_like(source)? {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    let first_unpacked = evaluated_args.len();
                    append_unpacked_call_arg_values(
                        source, &mut evaluated_args, &mut saw_named, context, values,
                    )?;
                    for argument in &mut evaluated_args[first_unpacked..] {
                        if argument.value.is_borrowed() {
                            argument.value = values.retain(argument.value)?;
                        }
                    }
                    Ok(())
                })();
                let released_array =
                    traversed.map_or(Ok(()), |array| eval_release_value(context, values, array));
                let released = release_expr_result(spread, context, values);
                unpacked.and(released_array).and(released)?;
                continue;
            }

            if let Some(name) = arg.name() {
                saw_named = true;
                let (value, ref_target) =
                    eval_call_arg_value(arg.value(), context, caller_scope, values)?;
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
            let (value, ref_target) = eval_call_arg_value(arg.value(), context, caller_scope, values)?;
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
        return Err(status);
    }
    Ok(evaluated_args)
}

/// Captures owned builtin inputs, using shared reference modes when a contract is supplied.
/// Without a contract, call_user_func captures independent values even for reference parameters.
pub(in crate::interpreter) fn eval_owned_builtin_call_arg_values(
    args: &[EvalCallArg], context: &mut ElephcEvalContext, caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps, owners: &mut Vec<RuntimeCellHandle>, evaluated: &mut Vec<EvaluatedCallArg>,
    contract: Option<&elephc_builtin_contract::BuiltinContract>,
) -> Result<(), EvalStatus> {
    evaluate_call_arguments(args, context, caller_scope, values, &mut |_, _| {}, Some(owners), evaluated, contract)
}

/// Evaluates source arguments with legacy targets or explicit owners selected by the parameter contract.
fn evaluate_call_arguments(
    args: &[EvalCallArg], context: &mut ElephcEvalContext, caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps, observe: &mut impl FnMut(&EvalExpr, RuntimeCellHandle),
    mut owners: Option<&mut Vec<RuntimeCellHandle>>, evaluated_args: &mut Vec<EvaluatedCallArg>,
    contract: Option<&elephc_builtin_contract::BuiltinContract>,
) -> Result<(), EvalStatus> {
    let mut saw_named = false;

    for (index, arg) in args.iter().enumerate() {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = if let Some(owners) = owners.as_deref_mut() {
                let spread = eval_owned_expr(arg.value(), context, caller_scope, values)?;
                owners.push(spread);
                spread
            } else { eval_expr(arg.value(), context, caller_scope, values)? };
            observe(arg.value(), spread);
            // A Traversable operand unpacks as the array its iteration yields.
            let spread = if values.type_tag(spread)? == EVAL_TAG_OBJECT {
                let array =
                    super::array_literals::eval_spread_traversable_array(spread, context, values)?;
                if let Some(owners) = owners.as_deref_mut() {
                    owners.push(array);
                }
                array
            } else {
                spread
            };
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            if let Some(owners) = owners.as_deref_mut() {
                if let Some(contract) = contract.filter(|contract| contract.params.iter().any(|param| param.by_ref)) {
                    builtin_arguments::append_spread(contract, spread, evaluated_args, &mut saw_named, context, values, owners)?;
                } else {
                    append_unpacked_value_call_args(spread, evaluated_args, &mut saw_named, context, values, owners)?;
                }
                let index = owners.iter().position(|value| *value == spread).expect("captured spread owner");
                owners.remove(index);
                context.clear_array_metadata(spread);
                eval_release_value(context, values, spread)?;
            } else {
                append_unpacked_call_arg_values(spread, evaluated_args, &mut saw_named, context, values)?;
            }
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let (value, ref_target) =
                evaluate_call_argument_value(arg.value(), context, caller_scope, values, owners.as_deref_mut(),
                    builtin_arguments::by_reference(contract, Some(name), evaluated_args.len()))?;
            let ref_target = call_arg_variable_target(arg.value(), ref_target, caller_scope);
            observe(arg.value(), value);
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
        let (value, ref_target) = evaluate_call_argument_value(arg.value(), context, caller_scope, values, owners.as_deref_mut(),
            builtin_arguments::by_reference(contract, None, evaluated_args.len()))
            .map_err(|status| trace_call_arg_error(index, arg.value(), status, context))?;
        let ref_target = call_arg_variable_target(arg.value(), ref_target, caller_scope);
        observe(arg.value(), value);
        evaluated_args.push(EvaluatedCallArg {
            name: None,
            value,
            ref_target,
        });
    }

    Ok(())
}

/// Captures an independent value or persistent reference owner, retaining legacy binding for unowned calls.
fn evaluate_call_argument_value(
    expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps, owners: Option<&mut Vec<RuntimeCellHandle>>,
    by_reference: bool,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if let Some(owners) = owners {
        if by_reference {
            let reference = builtin_arguments::reference(expr, context, scope, values)?;
            owners.push(reference);
            return Ok((reference, Some(EvalReferenceTarget::Cell { cell: reference })));
        }
        let value = eval_owned_expr(expr, context, scope, values)?;
        owners.push(value);
        Ok((value, None))
    } else { eval_call_arg_value(expr, context, scope, values) }
}

/// Names the CALLER'S OWN variable as the write-back target of a by-reference call argument.
///
/// `eval_call_arg_value` resolves a variable that is itself a reference (a by-reference
/// parameter, a `use (&$x)` capture) straight to the storage it aliases, which is what a closure
/// capture or a `foreach` binding needs because they outlive the statement. A call argument does
/// not: the call finishes before anything can rebind the name. Resolving it past the variable
/// meant the callee's result reached the aliased storage but never the variable the call was
/// written in, so `AA::pull($parameters, $k)` inside a closure left the closure's `$parameters`
/// stale, and its end-of-call write-back then put the OLD array back over the caller's.
/// `write_back_method_ref_target` follows the variable's own reference chain, so naming the
/// variable still reaches the aliased storage.
fn call_arg_variable_target(
    expr: &EvalExpr,
    target: Option<EvalReferenceTarget>,
    caller_scope: &mut ElephcEvalScope,
) -> Option<EvalReferenceTarget> {
    match (expr, target) {
        (EvalExpr::LoadVar(name), Some(_)) if caller_scope.reference_target(name).is_some() => {
            Some(EvalReferenceTarget::Variable {
                scope: crate::scope::EvalScopeRef::new(caller_scope),
                name: name.clone(),
            })
        }
        (_, target) => target,
    }
}

/// Returns whether an expression builds a cell nobody else holds.
///
/// These shapes allocate: a scalar literal, an array literal, an object construction, a clone. `eval_expr` answers
/// them with a fresh cell at one reference, while the generated bridge only borrows what it is
/// handed and `array_set` takes its own reference, so unless the consumer releases them the value
/// survives the whole process — that is why `->args([new Ref('x')])` never destructed its `Ref`.
/// Every other shape is deliberately absent: a variable, a property or a compound assignment
/// answers storage somebody else owns, and releasing that would free a live cell.
pub(in crate::interpreter) fn eval_expr_is_owning_temporary(expr: &EvalExpr) -> bool {
    matches!(
        expr,
        EvalExpr::Const(_)
            | EvalExpr::Array(_)
            | EvalExpr::NewObject { .. }
            | EvalExpr::DynamicNewObject { .. }
            | EvalExpr::NewAnonymousClass { .. }
            | EvalExpr::Clone(_)
    )
}

/// Returns the reference target for a property argument, or `None` when it cannot be bound.
///
/// An argument is prepared as a potential by-reference target before the callee is known, and
/// most callees take it by value. A readonly property (or an enum case's `name`/`value`) cannot
/// be bound by reference, but reading it for a by-value parameter is ordinary php:
/// Symfony's `CompiledAssetMapperConfigReader::configExists()` passes `$this->directory`, a
/// promoted readonly property, to `Path::join()`, and refusing it here killed every page that
/// renders an asset URL.
fn property_ref_target_if_bindable(
    object: RuntimeCellHandle,
    property: String,
    access_scope: ElephcEvalExecutionScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Option<EvalReferenceTarget> {
    validate_property_ref_target(object, &property, context, values).ok()?;
    Some(EvalReferenceTarget::ObjectProperty {
        object,
        property,
        access_scope,
    })
}

/// Emits the source argument that failed under opt-in runtime tracing.
fn trace_call_arg_error(
    index: usize,
    expr: &EvalExpr,
    status: EvalStatus,
    context: &ElephcEvalContext,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=call_arg_error index={index} status={status:?} file={:?} line={} expr={expr:?}",
            call_site.0,
            call_site.2,
        );
    }
    status
}

/// [`eval_call_arg_value`] for the by-reference output argument of a DIRECT builtin call
/// (`sort($a)`, `array_pop($a)`, `settype($a, ..)`, `preg_match(.., $m)`).
///
/// When the argument names a by-reference PARAMETER of the running function, the ordinary capture
/// resolves straight to the caller's variable, so the builtin's write lands there and the
/// function's own entry keeps the cell the write just released -- `function f(array &$a) {
/// sort($a); }` read freed storage afterwards and wrote it back at return. For such a variable
/// the target stays the LOCAL one: `eval_write_direct_ref_target` then writes through the
/// recorded reference and rebinds the local entry to the new value in one step.
///
/// NOT for the arguments of a call to a user function: the callee records the target as its own
/// parameter's reference, which can outlive this activation (`$this->attributes = &$attributes;`
/// in Symfony's `AttributeBag::initialize()`), and a target naming this activation's variable
/// would then dangle. A builtin's write is finished before the call returns.
pub(in crate::interpreter) fn eval_direct_ref_arg_value(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let (value, target) = eval_call_arg_value(expr, context, scope, values)?;
    if let EvalExpr::LoadVar(name) = expr {
        if scope.reference_target(name).is_some() {
            return Ok((
                value,
                Some(EvalReferenceTarget::Variable {
                    scope: crate::scope::EvalScopeRef::new(scope),
                    name: name.clone(),
                }),
            ));
        }
    }
    Ok((value, target))
}

/// Evaluates one call arg and captures caller-side storage for by-reference parameters.
pub(in crate::interpreter) fn eval_call_arg_value(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    match expr {
        EvalExpr::LoadVar(name) => {
            let value = visible_scope_cell(context, caller_scope, name)
                .map_or_else(|| values.null(), Ok)?;
            // `name` can ALREADY be a reference -- bound by `$name = &$expr;` or a by-reference
            // `foreach` -- in which case a fresh `Variable { scope, name }` node here would be a
            // SHALLOW alias of an alias: whatever captures this target (a closure's `use
            // (&$name)`, or a by-reference `foreach` binding its own loop variable from this
            // subject) would resolve it later by re-reading `name` out of `caller_scope`, not by
            // holding the cell `name` pointed at when THIS capture happened. Two failures follow
            // from that: a later rebind of `name` (a new loop iteration doing `$name = &$other[]`
            // again) silently redirects an already-captured reference to the NEW target instead
            // of the one it captured, and if `caller_scope` is a function or method activation
            // that returns before the capture is ever used -- exactly `use (&$x)` escaping the
            // defining call -- the raw pointer in `Variable` dereferences freed stack memory.
            // `write_back_method_ref_target`'s `Variable` arm already resolves this chain lazily,
            // at write-back time (`90c3d5a5ec`); resolving it eagerly here, while `caller_scope`
            // is still known good, is this function's mirror of that fix on the read side.
            let target = caller_scope
                .reference_target(name)
                .cloned()
                .unwrap_or_else(|| EvalReferenceTarget::Variable {
                    scope: crate::scope::EvalScopeRef::new(caller_scope),
                    name: name.clone(),
                });
            Ok((value, Some(target)))
        }
        EvalExpr::ArrayGet { array, index } => {
            // `$GLOBALS['name']` names a global variable, not an element of a local called
            // `GLOBALS`: `f($GLOBALS['x'])` passed null while `$v = $GLOBALS['x']` read the value.
            if is_globals_array(array) {
                let name = eval_global_name(index, context, caller_scope, values)?;
                let value = read_global_value(
                    &name,
                    context.quiet_property_fetch(),
                    context,
                    caller_scope,
                    values,
                )?;
                return Ok((value, None));
            }
            let EvalExpr::LoadVar(array_name) = array.as_ref() else {
                return eval_nested_array_element_call_arg_value(
                    array,
                    index,
                    context,
                    caller_scope,
                    values,
                );
            };
            let array = visible_scope_cell(context, caller_scope, array_name)
                .map_or_else(|| values.null(), Ok)?;
            let index_cell = eval_expr(index, context, caller_scope, values)?;
            let value = eval_array_get_result(array, index_cell, context, values)?;
            // The key's own cell is only read (for the element and its reference key); it was
            // never released, one cell per `&$x[$k]`.
            if values.type_tag(array)? == EVAL_TAG_OBJECT {
                release_expr_result(index_cell, context, values)?;
                return Ok((value, None));
            }
            let index = eval_array_reference_key(index_cell, values);
            release_expr_result(index_cell, context, values)?;
            let index = index?.ok_or(EvalStatus::RuntimeFatal)?;
            // An element of an array that is ITSELF a reference -- a by-reference parameter, a
            // `$session = &$_SESSION` -- is an element of what that reference names, and is
            // resolved now, as the `LoadVar` arm above resolves a variable, while `caller_scope`
            // is known good. Naming it through `caller_scope` kept a path into a scope that dies
            // with the call: Symfony's `NativeSessionStorage::loadSession(&$session)` binds each
            // bag's `$this->attributes = &$attributes` to `$session[$key]`, and every later
            // `$bag->set()` failed once `loadSession()` had returned.
            // The same holds for a `global`/superglobal alias (`$session = &$_SESSION`): the
            // element lives in the GLOBAL scope, under the global's own name.
            let global_alias = crate::interpreter::scope_cells::effective_global_alias_target(
                caller_scope,
                array_name,
            )
            .map(str::to_string);
            let global_scope = context
                .global_scope_ptr()
                .filter(|global| !std::ptr::eq(*global, caller_scope as *mut ElephcEvalScope));
            let target = match (caller_scope.reference_target(array_name).cloned(), global_alias) {
                (Some(array_target), _) => EvalReferenceTarget::NestedArrayElement {
                    array_target: Box::new(array_target),
                    index,
                },
                (None, Some(global_name)) => match global_scope.and_then(|global| unsafe { global.as_mut() }) {
                    Some(global) => EvalReferenceTarget::ArrayElement {
                        scope: crate::scope::EvalScopeRef::new(global),
                        array_name: global_name,
                        index,
                    },
                    None => EvalReferenceTarget::ArrayElement {
                        scope: crate::scope::EvalScopeRef::new(caller_scope),
                        array_name: global_name,
                        index,
                    },
                },
                (None, None) => EvalReferenceTarget::ArrayElement {
                    scope: crate::scope::EvalScopeRef::new(caller_scope),
                    array_name: array_name.clone(),
                    index,
                },
            };
            Ok((value, Some(target)))
        }
        EvalExpr::PropertyGet { object, property } => {
            let access_scope = context.execution_scope();
            let object = eval_expr(object, context, caller_scope, values)?;
            let value = eval_property_get_result(object, property, context, values)?;
            let target = property_ref_target_if_bindable(
                object,
                property.clone(),
                access_scope,
                context,
                values,
            );
            Ok((value, target))
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let access_scope = context.execution_scope();
            let object = eval_expr(object, context, caller_scope, values)?;
            let property = eval_dynamic_member_name(property, context, caller_scope, values)?;
            let value = eval_property_get_result(object, &property, context, values)?;
            let target =
                property_ref_target_if_bindable(object, property, access_scope, context, values);
            Ok((value, target))
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let access_scope = context.execution_scope();
            let class_name = resolve_eval_static_member_class_name(class_name, context)?;
            eval_static_property_call_arg_value(
                class_name,
                property.clone(),
                access_scope,
                context,
                values,
            )
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let access_scope = context.execution_scope();
            let class_name = eval_expr(class_name, context, caller_scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_call_arg_value(
                class_name,
                property.clone(),
                access_scope,
                context,
                values,
            )
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let access_scope = context.execution_scope();
            let class_name = eval_expr(class_name, context, caller_scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, caller_scope, values)?;
            eval_static_property_call_arg_value(
                class_name,
                property,
                access_scope,
                context,
                values,
            )
        }
        _ => eval_expr(expr, context, caller_scope, values).map(|value| (value, None)),
    }
}

/// Evaluates an array element whose array expression is itself a writable caller target.
fn eval_nested_array_element_call_arg_value(
    array: &EvalExpr,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let (array, array_target) = eval_call_arg_value(array, context, caller_scope, values)?;
    let index = eval_expr(index, context, caller_scope, values)?;
    let value = eval_array_get_result(array, index, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        return Ok((value, None));
    }
    let Some(array_target) = array_target else {
        return Ok((value, None));
    };
    let index = eval_array_reference_key(index, values)?.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((
        value,
        Some(EvalReferenceTarget::NestedArrayElement {
            array_target: Box::new(array_target),
            index,
        }),
    ))
}

/// Evaluates one static-property lvalue and records it as a by-reference call target.
fn eval_static_property_call_arg_value(
    class_name: String,
    property: String,
    access_scope: ElephcEvalExecutionScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let value = eval_static_property_get_result(&class_name, &property, context, values)?;
    Ok((
        value,
        Some(EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        }),
    ))
}

/// Converts a `call_user_func_array` argument array into ordered call arguments.
pub(in crate::interpreter) fn eval_array_call_arg_values(
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let len = values.array_len(arg_array)?;
    let mut evaluated_args = Vec::with_capacity(len);
    let mut saw_named = false;
    let unpacked = append_ordered_call_arg_values(
        arg_array,
        &mut evaluated_args,
        &mut saw_named,
        context,
        values,
        "Cannot use positional argument after named argument",
    );
    if let Err(status) = unpacked {
        for argument in evaluated_args {
            let _ = release_expr_result(argument.value, context, values);
        }
        return Err(status);
    }
    Ok(evaluated_args)
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
///
/// `array_get` answers an OWNED cell on every path — it increfs a stored boxed cell, or freshly
/// boxes a typed slot through `__rt_mixed_from_value`, which retains an object payload
/// (`src/codegen_support/runtime/objects/mixed_array_get.rs`). Releasing it here would hand the
/// callee freed storage on the typed-slot path, so the owning caller releases each element after
/// the call.
pub(in crate::interpreter) fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    append_ordered_call_arg_values(array, evaluated_args, saw_named, context, values,
        "Cannot use positional argument after named argument during unpacking")
}

/// Shares legacy array traversal while retaining the caller's PHP ordering diagnostic.
fn append_ordered_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    order_error: &str,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let ref_target = eval_array_reference_key(key, values)?
            .and_then(|key| context.array_element_alias(array, &key).cloned());
        let arg = match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    values.release(key)?;
                    return eval_throw_error(order_error, context, values);
                }
                let value = match values.array_get(array, key) {
                    Ok(value) => value,
                    Err(status) => {
                        values.release(key)?;
                        return Err(status);
                    }
                };
                // The compiled caller's by-reference argument is a marker CELL in the array slot,
                // which `array_get` dereferences into a plain value. Without recovering it a
                // by-ref closure parameter saw a copy: `tap($stack, fn (&$stack) => ...)` could
                // never update the caller's variable, and warned about it first.
                let ref_target = match ref_target {
                    Some(target) => Some(target),
                    None => values
                        .array_invoker_marker_at(array, position)?
                        .map(|(slot, source_tag)| EvalReferenceTarget::InvokerSlot { slot, source_tag }),
                };
                let (value, ref_target) =
                    eval_invoker_ref_arg_value_and_target(value, ref_target, values)?;
                EvaluatedCallArg {
                    name: None,
                    value,
                    ref_target,
                }
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = match String::from_utf8(name) {
                    Ok(name) => name,
                    Err(_) => {
                        values.release(key)?;
                        return Err(EvalStatus::RuntimeFatal);
                    }
                };
                let value = match values.array_get(array, key) {
                    Ok(value) => value,
                    Err(status) => {
                        values.release(key)?;
                        return Err(status);
                    }
                };
                let (value, ref_target) =
                    eval_invoker_ref_arg_value_and_target(value, ref_target, values)?;
                EvaluatedCallArg {
                    name: Some(name),
                    value,
                    ref_target,
                }
            }
            _ => {
                values.release(key)?;
                return Err(EvalStatus::RuntimeFatal);
            }
        };
        values.release(key)?;
        evaluated_args.push(arg);
    }
    Ok(())
}

/// Converts a descriptor-invoker ref marker into an eval-visible value and writeback target.
pub(in crate::interpreter) fn eval_invoker_ref_arg_value_and_target(
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_INVOKER_REF_CELL {
        return Ok((value, ref_target));
    }
    let slot = values.raw_value_word(value)? as usize;
    let source_tag = values.raw_value_high_word(value)?;
    let value = eval_invoker_slot_ref_target_value(slot, source_tag, values)?;
    Ok((
        value,
        ref_target.or(Some(EvalReferenceTarget::InvokerSlot { slot, source_tag })),
    ))
}
