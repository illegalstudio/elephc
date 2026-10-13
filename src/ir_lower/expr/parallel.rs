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

    let argument_values = lower_args_with_signature(ctx, Some(&signature), &args[1..]);
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
    let payload_values = sources
        .iter()
        .filter_map(|(_, cancellation, _, value)| (!cancellation).then_some(*value))
        .collect::<Vec<_>>();
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
    let job_id = ctx.emit_value(
        Op::ParallelSpawn,
        vec![serialized.value],
        Some(Immediate::Data(callback)),
        PhpType::Int,
        Op::ParallelSpawn.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, serialized, Some(expr.span));
    crate::ir_lower::ownership::release_if_owned(ctx, task_value, Some(expr.span));

    let future = emit_fixed_object_new(
        ctx,
        "Elephc\\Parallel\\Future",
        vec![job_id.value],
        PhpType::Object("Elephc\\Parallel\\Future".to_string()),
        expr.span,
    );
    let record = ctx.intern_string("__recordFuture");
    ctx.emit_void(
        Op::MethodCall,
        vec![group.value, future.value],
        Some(Immediate::Data(record)),
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, group, Some(expr.span));
    future
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
