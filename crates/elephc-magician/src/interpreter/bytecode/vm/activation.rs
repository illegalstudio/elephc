//! Purpose:
//! Binds explicit bytecode call frames and balances their scopes and PHP call metadata.
//!
//! Called from:
//! - `super::step()` for calls, declarations and returns.
//! - `super::run_until_baseline()` to unwind a failed activation.
//!
//! Key details:
//! - Boxed scopes keep backtrace argument pointers stable as the VM stack grows.
//! - Slow calls keep source arguments for the shared dispatcher and its reference modes.

use super::*;

/// Resolves a call before argument effects, retaining a fast body or dispatching source arguments.
pub(super) fn prepare_call(
    vm: &mut Vm,
    index: u16,
    fallback_end: u32,
    context: &mut ElephcEvalContext,
    external: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let body = current_body(vm)?;
    let site = body.calls.get(index as usize).ok_or(EvalStatus::RuntimeFatal)?;
    let key = site.name.to_ascii_lowercase();
    // Catalog names stay on the shared dispatcher, including unavailable capabilities.
    // Resolving here also preserves reference modes and undefined-callee error timing.
    let source_dispatch = eval_expr_language_construct_name(&key)
        || elephc_builtin_contract::lookup(&key).is_some()
        || context.generic_function_template(&key).is_some();
    let fast = if source_dispatch { None } else {
        context.function(&key).and_then(|function| {
            let body = function.bytecode()?;
            (body.fast_positional && body.params.len() == site.args.len()).then(|| Arc::clone(body))
        })
    };
    if let Some(body) = fast {
        vm.prepared_calls.push(body);
    } else {
        let slot = vm.frames.last().ok_or(EvalStatus::RuntimeFatal)?.scope_slot;
        let scope = frame_scope(slot, &mut vm.scopes, external);
        let result = eval_call(&site.name, &site.args, context, scope, values)?;
        let result = if result.is_borrowed() { copy_scope_value(result, context, values)? } else { result };
        vm.stack.push(result);
        set_ip(vm, fallback_end)?;
    }
    Ok(())
}

/// Moves evaluated operands into the compiled body selected before argument evaluation.
pub(super) fn call(
    vm: &mut Vm,
    argc: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let body = vm.prepared_calls.pop().ok_or(EvalStatus::RuntimeFatal)?;
    let base = vm.stack.len().checked_sub(argc).ok_or(EvalStatus::RuntimeFatal)?;
    let args = vm.stack.split_off(base);
    push_fast_call(vm, body, args, context, values)
}

/// Binds positional arguments and pushes a frame the current loop will run.
fn push_fast_call(
    vm: &mut Vm,
    body: Arc<CompiledBody>,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut function_scope = Box::new(ElephcEvalScope::new());
    let mut pending = args.into_iter();
    for name in body.params.iter() {
        let Some(value) = pending.next() else { break };
        if let Err(status) = store_into(&mut function_scope, name, value, context, values) {
            let _ = release_cells(&pending.collect::<Vec<_>>(), context, values);
            let _ = finish_activation_scope(&mut function_scope, Err(status), context, values);
            return Err(status);
        }
    }
    context.push_function(&body.name);
    context.push_class_scope(String::new());
    context.push_called_class_scope(String::new());
    let mut arguments = EvalFunctionArgsFrame::new(body.params.clone(), body.params.len(), Vec::new());
    arguments.bind_scope(&function_scope);
    context.push_function_args(arguments);
    let scope_slot = vm.scopes.len();
    vm.scopes.push(function_scope);
    let stack_base = vm.stack.len();
    vm.push_frame(Frame {
        body,
        ip: 0,
        stack_base,
        scope_slot: Some(scope_slot),
        pushed_activation: true,
    });
    Ok(())
}

/// Defines a compiled function when its declaration statement is reached.
pub(super) fn define_fn(vm: &mut Vm, index: u16, context: &mut ElephcEvalContext) -> Result<(), EvalStatus> {
    let body = current_body(vm)?
        .defined
        .get(index as usize)
        .cloned()
        .ok_or(EvalStatus::RuntimeFatal)?;
    if context.generic_function_template(&body.name).is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut function = EvalFunction::new(body.name.clone(), body.params.clone(), body.source_body.clone());
    if let Some(location) = body.source_location {
        function = function.with_source_location(location);
    }
    function.set_bytecode(Arc::clone(&body));
    let key = body.name.to_ascii_lowercase();
    context.define_function(key, function).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(())
}

/// Leaves the current frame, pushing a value for an inner caller.
pub(super) fn leave_frame(
    vm: &mut Vm,
    value: Option<RuntimeCellHandle>,
    baseline: usize,
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let stack_base = vm.frames.last().ok_or(EvalStatus::RuntimeFatal)?.stack_base;
    if let Err(status) = release_span(&mut vm.stack, stack_base, context, values) {
        if let Some(value) = value {
            let _ = eval_release_value(context, values, value);
        }
        return Err(status);
    }
    let frame = vm.frames.pop().ok_or(EvalStatus::RuntimeFatal)?;
    let returned = if frame.pushed_activation {
        let mut function_scope = vm.scopes.pop().ok_or(EvalStatus::RuntimeFatal)?;
        let passed = match value {
            Some(value) => Ok(value),
            None => values.null(),
        };
        let passed = release_function_args(passed, context, values);
        context.pop_called_class_scope();
        context.pop_class_scope();
        let finished = match passed {
            Ok(passed) => finish_activation_scope(&mut function_scope, Ok(passed), context, values),
            Err(status) => {
                let _ = finish_activation_scope(&mut function_scope, Err(status), context, values);
                Err(status)
            }
        };
        context.pop_function();
        Some(finished?)
    } else {
        value
    };
    if vm.frames.len() == baseline {
        vm.result = Some(match returned {
            Some(value) => EvalControl::Return(value),
            None => EvalControl::ReturnVoid,
        });
    } else if let Some(value) = returned {
        vm.stack.push(value);
    } else {
        vm.stack.push(values.null()?);
    }
    Ok(())
}

/// Releases frames above `baseline` after a runtime error.
pub(super) fn unwind(vm: &mut Vm, baseline: usize, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) {
    while vm.frames.len() > baseline {
        let frame = vm.frames.pop().unwrap();
        let _ = release_span(&mut vm.stack, frame.stack_base, context, values);
        if frame.pushed_activation {
            if let Some(mut function_scope) = vm.scopes.pop() {
                let _ = release_function_args(Err(EvalStatus::RuntimeFatal), context, values);
                context.pop_called_class_scope();
                context.pop_class_scope();
                let _ = finish_activation_scope(&mut function_scope, Err(EvalStatus::RuntimeFatal), context, values);
                context.pop_function();
            }
        }
    }
    vm.prepared_calls.clear();
    vm.result = None;
}
