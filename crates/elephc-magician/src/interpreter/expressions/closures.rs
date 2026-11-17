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
    let mut owners = Vec::with_capacity(captures.len());
    for capture in captures {
        if capture.implicit() && visible_scope_cell(context, scope, capture.name()).is_none() {
            continue;
        }
        let binding = match eval_closure_capture(capture, context, scope, values) {
            Ok(binding) => binding,
            Err(status) => return finish_eval_argument_values(Err(status), owners, context, values),
        };
        owners.push(binding.value());
        // Metadata borrows the runtime edge acquired below. Keeping this retain in the
        // context registry would strand every captured value after its Closure is freed.
        bindings.push(EvalClosureCaptureBinding::new(
            binding.name(), binding.value().borrowed(), binding.by_ref_target().cloned(),
        ));
    }
    let creating_class = context.current_class_scope().map(str::to_string);
    let mut closure = EvalClosure::new(function.clone(), bindings, is_static).with_creating_class(creating_class.as_deref());
    closure.set_declaring_class_scopes(
        context.current_class_scope().map(str::to_string),
        context.current_called_class_scope().map(str::to_string),
    );
    // PHP binds `$this` into a non-static closure or arrow function automatically when one is
    // declared inside a method body while an instance is in scope -- this is not a `use (...)`
    // capture (and an arrow function's implicit capture list explicitly excludes `this`), so it
    // has to be captured here, at the declaration site, the same way the class scopes above are.
    // A `static function`/`static fn` never gets `$this`, even if one happens to be in scope
    // (e.g. a static closure declared inside another closure that itself captured `$this`).
    if !is_static {
        if let Some(this_value) = visible_scope_cell(context, scope, "this") {
            let retained = match values.retain(this_value) {
                Ok(retained) => retained,
                Err(status) => return finish_eval_argument_values(Err(status), owners, context, values),
            };
            closure.set_declaring_this(Some(retained));
        }
    }
    closure.set_declaring_call_site(context.call_site());
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=closure_create function={:?} lexical_class={:?} called_class={:?}",
            function.name(),
            closure.declaring_class_scope(),
            closure.declaring_called_class_scope(),
        );
    }
    let name = context.define_closure(closure);
    let object = eval_closure_object_expr(EvalClosureObjectTarget::Named(name), context, values);
    finish_eval_argument_values(object, owners, context, values)
}

/// Materializes one PHP-visible `Closure` object for an eval callable target.
pub(in crate::interpreter) fn eval_closure_object_expr(
    mut target: EvalClosureObjectTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    let attached = (|| {
        let mut children = Vec::new();
        if let Some(receiver) = target.receiver_mut() {
            children.push(*receiver);
            // Metadata borrows the cell owned by the runtime edge, not its caller's scope.
            *receiver = receiver.borrowed();
        }
        // Bound copies need their own edges too: the original Closure may die first.
        if let EvalClosureObjectTarget::Named(name) | EvalClosureObjectTarget::BoundNamed { name, .. } = &target {
            if let Some(closure) = context.closure(name) {
                children.extend(closure.captures().iter().map(EvalClosureCaptureBinding::value));
            }
        }
        values.retain_object_children(object, &children)?;
        values.object_identity(object)
    })();
    let identity = match attached {
        Ok(identity) => identity,
        Err(status) => {
            let _ = values.release(object);
            return Err(status);
        }
    };
    context.register_closure_object_target(identity, target);
    crate::ffi::dynamic_destructors::register_dynamic_object_context(identity, context);
    Ok(object)
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
        // The closure OWNS one reference to the captured cell, as a by-value capture does. It
        // routinely outlives the defining activation (`return function () use (&$x) {...};`),
        // and that activation releases its own reference when it returns. An undefined
        // variable comes back as a fresh null, which is already owned.
        let defined = visible_scope_cell(context, scope, capture.name()).is_some();
        let expr = EvalExpr::LoadVar(capture.name().to_string());
        let (value, target) = eval_call_arg_value(&expr, context, scope, values).map_err(|status| {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=closure_capture_error capture={:?} by_ref=true status={status:?}",
                    capture.name(),
                );
            }
            status
        })?;
        let target = match target {
            Some(mut target) => {
                retain_capture_target_object(&mut target, values)?;
                Some(target)
            }
            None => None,
        };
        let value = if defined { values.retain(value)? } else { value };
        // The variable may itself be a reference (a by-reference parameter): `target` is then
        // the variable it aliases, so keep this scope's own entry as a second holder.
        let defining_target = (scope.reference_target(capture.name()).is_some() && defined).then(
            || EvalReferenceTarget::Variable {
                scope: crate::scope::EvalScopeRef::new(scope),
                name: capture.name().to_string(),
            },
        );
        return Ok(
            EvalClosureCaptureBinding::new(capture.name(), value, target)
                .with_defining_target(defining_target),
        );
    }
    let value = if let Some(value) = visible_scope_cell(context, scope, capture.name()) {
        if values.is_reference(value)? { values.copy_value(value)? } else { values.retain(value)? }
    } else {
        values.null()?
    };
    Ok(EvalClosureCaptureBinding::new(capture.name(), value, None))
}

/// Gives a by-reference capture rooted in an object property a reference to that object's cell.
///
/// The target names the receiver through the cell the method ran on, and that cell is the call's
/// own copy of the caller's, released when the call returns. A non-static closure keeps it alive
/// through `$this`; a `static` one holds nothing, so a capture outliving the method wrote through
/// a freed cell. The closure owns this reference the way it owns the `$this` it captures.
fn retain_capture_target_object(
    target: &mut EvalReferenceTarget,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match target {
        EvalReferenceTarget::ObjectProperty { object, .. } => {
            *object = values.retain(*object)?;
            Ok(())
        }
        EvalReferenceTarget::NestedArrayElement { array_target, .. } => {
            retain_capture_target_object(array_target, values)
        }
        _ => Ok(()),
    }
}
