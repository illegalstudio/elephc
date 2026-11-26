//! Purpose:
//! Publishes eval closure captures and bound receivers as native object lifetime edges.
//!
//! Called from:
//! - Eval closure expressions, first-class callables and Closure binding helpers.
//!
//! Key details:
//! - Context metadata borrows captures owned by each runtime Closure object.
//! - Construction leases retire on success and every partial failure.

use super::*;
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};
use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};

/// Materializes one eval closure literal as a PHP-visible `Closure` object.
pub(super) fn eval_closure_expr(
    function: &EvalFunction,
    captures: &[crate::eval_ir::EvalClosureCapture],
    is_static: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bindings = Vec::with_capacity(captures.len());
    let owners = StagedRuntimeOwners::default();
    with_runtime_owners(&owners, || {
        let object = (|| {
            for capture in captures {
                let binding = eval_closure_capture(capture, context, scope, values)?;
                owners.stage(binding.value());
                // Metadata borrows the runtime edge acquired below. Keeping this retain in the
                // context registry would strand every captured value after its Closure is freed.
                bindings.push(EvalClosureCaptureBinding::new(
                    binding.name(), binding.value().borrowed(), binding.by_ref_target().cloned(),
                ));
            }
            let creating_class = context.current_class_scope().map(str::to_string);
            let closure = EvalClosure::new(function.clone(), bindings, is_static)
                .with_creating_class(creating_class.as_deref());
            let name = context.define_closure(closure);
            eval_closure_object_expr(EvalClosureObjectTarget::Named(name), context, values)
        })();
        let owners = owners.take_all();
        finish_eval_owner_batch(
            &owners,
            object,
            ReleaseOrder::Forward,
            CleanupErrorPolicy::First,
            context,
            values,
        )
    })
}

/// Materializes one PHP-visible `Closure` object for an eval callable target.
pub(in crate::interpreter) fn eval_closure_object_expr(
    mut target: EvalClosureObjectTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = RuntimeOwnerSlot::new(values.new_object("stdClass")?);
    with_runtime_owners(&object, || {
        let attached = (|| {
            let mut children = Vec::new();
            if let Some(receiver) = target.receiver_mut() {
                children.push(*receiver);
                // Metadata borrows the cell owned by the runtime edge, not its caller's scope.
                *receiver = receiver.borrowed();
            }
            // Bound copies need their own edges too: the original Closure may die first.
            if let EvalClosureObjectTarget::Named(name)
                | EvalClosureObjectTarget::BoundNamed { name, .. } = &target
            {
                if let Some(closure) = context.closure(name) {
                    children.extend(closure.captures().iter().map(EvalClosureCaptureBinding::value));
                }
            }
            values.retain_object_children(object.borrowed(), &children)?;
            values.object_identity(object.borrowed())
        })();
        let operation = attached.map(|identity| {
            context.register_closure_object_target(identity, target);
            crate::ffi::dynamic_destructors::register_dynamic_object_context(identity, context);
        });
        object.finish(operation, |object| values.release(object))
    })
}

/// Evaluates one closure capture from the defining scope.
fn eval_closure_capture(
    capture: &crate::eval_ir::EvalClosureCapture,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureCaptureBinding, EvalStatus> {
    if capture.by_ref() {
        if let Some(reference) = eval_persistent_variable_reference(capture.name(), context, scope, values)? {
            let reference = values.retain(reference)?;
            return Ok(EvalClosureCaptureBinding::new(capture.name(), reference, Some(EvalReferenceTarget::Cell { cell: reference })));
        }
        let expr = EvalExpr::LoadVar(capture.name().to_string());
        let (value, target) = eval_call_arg_value(&expr, context, scope, values)?;
        return Ok(EvalClosureCaptureBinding::new(
            capture.name(),
            value,
            target,
        ));
    }
    let value = if let Some(value) = visible_scope_cell(context, scope, capture.name()) {
        if values.is_reference(value)? { values.copy_value(value)? } else { values.retain(value)? }
    } else {
        values.null()?
    };
    Ok(EvalClosureCaptureBinding::new(capture.name(), value, None))
}
