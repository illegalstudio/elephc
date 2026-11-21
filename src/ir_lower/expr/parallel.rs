//! Purpose:
//! Lowers `Elephc\Parallel\TaskGroup::spawn()` into transferable payload and worker operations.
//!
//! Called from:
//! - `crate::ir_lower::expr::method_calls::lower_method_call()` for the locked Parallel surface.
//!
//! Key details:
//! - Arguments and captures are evaluated locally, copied into `[arguments..., captures...]`, then
//!   serialized before any worker can start.
//! - Only statically resolved Closure targets reach this path: literal closures, user functions,
//!   builtins, and extern functions. The checker owns transferability and rejects unsupported
//!   receiver-bound, static-method, and variadic task shapes.
//! - The returned `Future` stores only a native job id and is registered by its constructor.

use super::*;

/// A checked entry plus an immutable copied-input template, independent of any submitted job.
pub(super) struct PreparedParallelSubmission {
    pub serialized: LoweredValue,
    pub callback: crate::ir::DataId,
    pub task: LoweredValue,
    pub control: Option<LoweredValue>,
}

/// Keeps the checked static worker target while wrapping its engine handle in the unified Task type.
pub(super) fn lower_unified_parallel_spawn(
    ctx: &mut LoweringContext<'_, '_>,
    group: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let pinned = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, group, Some(expr.span));
    let execution = lower_expr(ctx, &args[0]);
    let parallel_name = ctx.intern_string("__parallelScope");
    let parallel = ctx.emit_value(Op::MethodCall, vec![pinned.value], Some(Immediate::Data(parallel_name)),
        PhpType::Object("Elephc\\Parallel\\TaskGroup".into()), Op::MethodCall.default_effects(), Some(expr.span));
    let future = lower_parallel_spawn(ctx, parallel, &args[1..], expr);
    crate::ir_lower::ownership::release_if_owned(ctx, parallel, Some(expr.span));
    let async_name = ctx.intern_string("__asyncScope");
    let coordinator = ctx.emit_value(Op::MethodCall, vec![pinned.value], Some(Immediate::Data(async_name)),
        PhpType::Object("Elephc\\Async\\TaskGroup".into()), Op::MethodCall.default_effects(), Some(expr.span));
    let null = ctx.emit_value(Op::ConstNull, vec![], None, PhpType::Void,
        Op::ConstNull.default_effects(), Some(expr.span));
    let task = emit_fixed_object_new(ctx, "Elephc\\Task\\Task",
        vec![coordinator.value, null.value, future.value], PhpType::Object("Elephc\\Task\\Task".into()), expr.span);
    let record = ctx.intern_string("__record");
    ctx.emit_void(Op::MethodCall, vec![pinned.value, task.value], Some(Immediate::Data(record)),
        Op::MethodCall.default_effects(), Some(expr.span));
    // Object construction already retires its owning argument temporaries.
    // Releasing Future/coordinator again would consume the references stored in Task.
    for value in [execution, pinned] {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    task
}

/// Serializes evaluated arguments/captures, submits a checked worker entry and records its Future.
pub(super) fn lower_parallel_spawn(
    ctx: &mut LoweringContext<'_, '_>,
    group: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    // The receiver can come from a flow-narrowed value (for example a TaskGroup
    // yielded from a suspended Fiber) whose source slot is Mixed. Retain it across
    // task/argument evaluation and both bookkeeping calls, then let the caller
    // release any temporary source load after this lowering completes.
    let group = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(
        ctx,
        group,
        Some(expr.span),
    );
    let prepared = prepare_parallel_submission(ctx, group, args, expr, None);
    let job_id = ctx.emit_value(
        Op::ParallelSpawn, vec![prepared.serialized.value], Some(Immediate::Data(prepared.callback)),
        PhpType::Int, Op::ParallelSpawn.default_effects(), Some(expr.span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, prepared.serialized, Some(expr.span));
    crate::ir_lower::ownership::release_if_owned(ctx, prepared.task, Some(expr.span));
    let future = emit_fixed_object_new(ctx, "Elephc\\Parallel\\Future", vec![job_id.value],
        PhpType::Object("Elephc\\Parallel\\Future".into()), expr.span);
    let record = ctx.intern_string("__recordFuture");
    ctx.emit_void(Op::MethodCall, vec![group.value, future.value], Some(Immediate::Data(record)),
        Op::MethodCall.default_effects(), Some(expr.span));
    crate::ir_lower::ownership::release_if_owned(ctx, group, Some(expr.span));
    future
}

/// Freezes checked task input/captures and emits its worker entry without starting a worker.
/// The caller pins the scope across preparation and any subsequent job registration.
pub(super) fn prepare_parallel_submission(
    ctx: &mut LoweringContext<'_, '_>,
    group: LoweredValue,
    args: &[Expr],
    expr: &Expr,
    periodic: Option<(LoweredValue, LoweredValue)>,
) -> PreparedParallelSubmission {
    let Some(task_expr) = args.first() else {
        unreachable!("Parallel TaskGroup::spawn arity is checked before EIR lowering");
    };
    let tracked = static_callable_binding_for_expr(ctx, task_expr);
    let task_value = lower_expr(ctx, task_expr);
    let binding = tracked.or_else(|| ctx.take_pending_static_callable_result());
    let (task_name, signature, captures, register_task_function) = match binding {
        Some(StaticCallableBinding::Closure {
            name,
            signature,
            captures,
            ..
        }) => (name, signature, captures, true),
        Some(StaticCallableBinding::UserFunction(name)) => {
            let signature = ctx
                .functions
                .get(&name)
                .cloned()
                .expect("checked Parallel function Closure must retain its signature");
            (name, signature, Vec::new(), true)
        }
        Some(StaticCallableBinding::Builtin(name)) => {
            let signature = call_signature(
                ctx,
                &name,
                source_prefers_extension_builtin(&name),
            )
            .expect("checked Parallel builtin Closure must retain its signature");
            (name, signature, Vec::new(), false)
        }
        Some(StaticCallableBinding::ExternFunction(name)) => {
            let signature = ctx
                .extern_functions
                .get(&name)
                .map(function_sig_from_extern_for_descriptor)
                .expect("checked Parallel extern Closure must retain its signature");
            (name, signature, Vec::new(), false)
        }
        _ => panic!("checked Parallel spawn must retain a supported static Closure binding"),
    };

    let mut caller_signature = signature.clone();
    if periodic.is_some() && !caller_signature.params.is_empty() {
        caller_signature.params.remove(0);
        remove_injected_parameter_metadata(&mut caller_signature.param_type_exprs);
        remove_injected_parameter_metadata(&mut caller_signature.param_attributes);
        remove_injected_parameter_metadata(&mut caller_signature.defaults);
        remove_injected_parameter_metadata(&mut caller_signature.ref_params);
        remove_injected_parameter_metadata(&mut caller_signature.declared_params);
    }
    let mut argument_values = lower_args_with_signature(ctx, Some(&caller_signature), &args[1..]);
    // Keep the compiler-only spawn path subject to the same lexical-scope lifetime as the
    // source-level method. This is deliberately after task/argument evaluation, matching PHP's
    // receiver-then-argument evaluation order, and before any payload or native job exists.
    let assert_open = ctx.intern_string("__assertSpawnable");
    ctx.emit_void(
        Op::MethodCall,
        vec![group.value],
        Some(Immediate::Data(assert_open)),
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    let control = periodic.map(|(hub, seconds)| {
        let create = ctx.intern_string("__createPeriodicControl");
        let control = ctx.emit_value(Op::MethodCall, vec![hub.value, seconds.value],
            Some(Immediate::Data(create)), PhpType::Object("Elephc\\Task\\PeriodicControl".into()),
            Op::MethodCall.default_effects(), Some(expr.span));
        argument_values.insert(0, control.value);
        control
    });
    let argument_sources = argument_values.iter().enumerate().map(|(index, value)| {
        let php_type = ctx.builder.value_php_type(*value);
        let cancellation = matches!(
            php_type.codegen_repr(),
            PhpType::Object(ref class) if crate::types::parallel_transfer::is_parallel_cancellation(class)
        );
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        (php_type, cancellation, by_ref, *value)
    });
    let capture_sources = captures.iter().map(|capture| {
        let php_type = ctx.builder.value_php_type(capture.value);
        let cancellation = matches!(
            php_type.codegen_repr(),
            PhpType::Object(ref class) if crate::types::parallel_transfer::is_parallel_cancellation(class)
        );
        (php_type, cancellation, capture.by_ref_local.is_some(), capture.value)
    });
    let sources = argument_sources.chain(capture_sources).collect::<Vec<_>>();
    let mut payload_values = Vec::new();
    for (php_type, cancellation, _, value) in &sources {
        if *cancellation { continue; }
        if matches!(php_type.codegen_repr(), PhpType::Object(ref class)
            if crate::types::parallel_transfer::is_periodic_control(class))
        {
            let method = ctx.intern_string("__nativeId");
            let id = ctx.emit_value(Op::MethodCall, vec![*value], Some(Immediate::Data(method)),
                PhpType::Int, Op::MethodCall.default_effects(), Some(expr.span));
            payload_values.push(id.value);
            if control.is_none_or(|control| control.value != *value) {
                let source = LoweredValue { value: *value, ir_type: ctx.builder.value_type(*value) };
                if ctx.value_needs_release_after_use(source) {
                    crate::ir_lower::ownership::release_if_owned(ctx, source, Some(expr.span));
                }
            }
        } else { payload_values.push(*value); }
    }
    let payload_capacity = payload_values.len();
    let payload = mixed_array_from_values(
        ctx,
        payload_values.into_iter(),
        payload_capacity,
        expr.span,
    );
    let serialized = emit_builtin_call_value(
        ctx,
        "serialize",
        vec![payload.value],
        PhpType::Str,
        expr.span,
        None,
    );

    let worker_name = format!("{}__parallel_worker", ctx.next_closure_name());
    function::lower_parallel_worker_function(
        ctx,
        &worker_name,
        &task_name,
        &signature,
        &sources
            .iter()
            .map(|(php_type, cancellation, by_ref, _)| {
                (php_type.clone(), *cancellation, *by_ref)
            })
            .collect::<Vec<_>>(),
        register_task_function,
    );
    let callback = ctx.intern_function_name(&worker_name);
    PreparedParallelSubmission { serialized, callback, task: task_value, control }
}

/// Optional per-parameter tables may be empty even when a callable has visible parameters.
fn remove_injected_parameter_metadata<T>(metadata: &mut Vec<T>) {
    if !metadata.is_empty() { metadata.remove(0); }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    /// Removing an injected control preserves absent tables and shifts populated ones exactly once.
    #[test]
    fn periodic_signature_optional_metadata_can_be_empty() {
        let mut absent: Vec<bool> = Vec::new();
        remove_injected_parameter_metadata(&mut absent);
        assert!(absent.is_empty());
        let mut present = vec![false, true, false];
        remove_injected_parameter_metadata(&mut present);
        assert_eq!(present, [true, false]);
    }
}

/// Creates the repeated-worker factory only after checked registration arguments are evaluated.
pub(super) fn lower_parallel_every(
    ctx: &mut LoweringContext<'_, '_>, hub: LoweredValue, args: &[Expr], expr: &Expr,
) -> LoweredValue {
    let hub = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, hub, Some(expr.span));
    let execution = lower_expr(ctx, &args[0]);
    let seconds = lower_expr(ctx, &args[1]);
    let method = ctx.intern_string("__parallelScope");
    let scope = ctx.emit_value(Op::MethodCall, vec![hub.value], Some(Immediate::Data(method)),
        PhpType::Object("Elephc\\Parallel\\TaskGroup".into()), Op::MethodCall.default_effects(), Some(expr.span));
    let prepared = prepare_parallel_submission(ctx, scope, &args[2..], expr, Some((hub, seconds)));
    let control = prepared.control.expect("periodic preparation creates one control");
    let factory = function::lower_parallel_submission_factory(ctx, prepared.serialized, prepared.callback, expr.span);
    let register = ctx.intern_string("__registerParallelEvery");
    ctx.emit_void(Op::MethodCall, vec![hub.value, control.value, factory.value], Some(Immediate::Data(register)),
        Op::MethodCall.default_effects(), Some(expr.span));
    for value in [factory, prepared.serialized, prepared.task, scope, execution, hub] {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    control
}

/// Builds the ordered boxed transfer payload and retires source operands after retaining insertion.
fn mixed_array_from_values(
    ctx: &mut LoweringContext<'_, '_>,
    values: impl Iterator<Item = crate::ir::ValueId>,
    capacity: usize,
    span: Span,
) -> LoweredValue {
    let element_type = PhpType::Mixed;
    let array_type = PhpType::Array(Box::new(element_type.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(capacity as u32)),
        array_type,
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    for value in values {
        let lowered = LoweredValue {
            value,
            ir_type: ctx.builder.value_type(value),
        };
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value],
            None,
            Op::ArrayPush.default_effects(),
            Some(span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(
            ctx,
            Some(&element_type),
            lowered,
            span,
        );
    }
    array
}
