//! Purpose:
//! Materializes PHP first-class callable expressions into eval Closure objects.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()` through `expressions::calls`.
//!
//! Key details:
//! - Object and static-method callables validate visibility, abstract methods,
//!   late-static receiver metadata, and generated/AOT bridge scope before capture.

use super::*;

/// Resolves a first-class function callable name with PHP namespace fallback rules.
pub(in crate::interpreter) fn eval_function_callable_expr(
    name: &str,
    fallback_name: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_function_probe_exists(context, name) {
        return eval_closure_object_expr(
            EvalClosureObjectTarget::Named(name.trim_start_matches('\\').to_ascii_lowercase()),
            context,
            values,
        );
    }
    if let Some(fallback_name) = fallback_name {
        if eval_function_probe_exists(context, fallback_name) {
            return eval_closure_object_expr(
                EvalClosureObjectTarget::Named(
                    fallback_name.trim_start_matches('\\').to_ascii_lowercase(),
                ),
                context,
                values,
            );
        }
    }
    eval_throw_error(
        &format!("Call to undefined function {}()", name.trim_start_matches('\\')),
        context,
        values,
    )
}

/// Materializes an invokable-object first-class callable as a PHP `Closure` object.
pub(in crate::interpreter) fn eval_invokable_callable_expr(
    object: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = eval_expr(object, context, scope, values)?;
    eval_invokable_object_precheck(object, context, values)?;
    eval_closure_object_expr(
        EvalClosureObjectTarget::InvokableObject { object },
        context,
        values,
    )
}

/// Materializes an object method first-class callable and records captured AOT bridge scope.
pub(in crate::interpreter) fn eval_method_callable_expr(
    object: &EvalExpr,
    method: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = eval_expr(object, context, scope, values)?;
    let method = eval_dynamic_member_name(method, context, scope, values)?;
    let target = eval_method_callable_target(object, method, context, values)?;
    eval_closure_object_expr(target, context, values)
}

/// Validates and builds the retained target for an object method first-class callable.
fn eval_method_callable_target(
    object: RuntimeCellHandle,
    method_name: String,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(EvalClosureObjectTarget::ObjectMethod {
            object,
            method: method_name,
            called_class: None,
            native_class: None,
            bridge_scope: None,
        });
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        return eval_native_object_method_callable_target(
            object,
            &class_name,
            &class_name,
            method_name,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    if let Some((declaring_class, method)) =
        eval_dynamic_method_for_call(&called_class_name, &method_name, context)
    {
        if method.is_abstract() {
            return eval_first_class_abstract_method_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err()
            && !eval_instance_magic_callable_for_class(&called_class_name, context)
        {
            return eval_first_class_method_access_error(
                &declaring_class,
                method.name(),
                method.visibility(),
                context,
                values,
            );
        }
        return Ok(EvalClosureObjectTarget::ObjectMethod {
            object,
            method: method_name,
            called_class: Some(called_class_name),
            native_class: None,
            bridge_scope: None,
        });
    }
    if let Some(parent) = context.class_native_parent_name(&called_class_name) {
        return eval_native_object_method_callable_target(
            object,
            &parent,
            &called_class_name,
            method_name,
            context,
            values,
        );
    }
    if eval_instance_magic_callable_for_class(&called_class_name, context) {
        return Ok(EvalClosureObjectTarget::ObjectMethod {
            object,
            method: method_name,
            called_class: Some(called_class_name),
            native_class: None,
            bridge_scope: None,
        });
    }
    eval_first_class_undefined_method_error(&called_class_name, &method_name, context, values)
}

/// Validates generated/AOT object-method first-class callable metadata.
fn eval_native_object_method_callable_target(
    object: RuntimeCellHandle,
    native_class: &str,
    called_class_name: &str,
    method_name: String,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let Some((declaring_class, visibility, _, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(native_class, &method_name, context, values)?
    else {
        if eval_native_instance_magic_callable_for_class(native_class, context, values)?
            || !values.class_exists(native_class)?
        {
            return Ok(EvalClosureObjectTarget::ObjectMethod {
                object,
                method: method_name,
                called_class: Some(called_class_name.to_string()),
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            called_class_name,
            &method_name,
            context,
            values,
        );
    };
    if is_abstract {
        return eval_first_class_abstract_method_error(
            &declaring_class,
            &method_name,
            context,
            values,
        );
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        if eval_native_instance_magic_callable_for_class(native_class, context, values)? {
            return Ok(EvalClosureObjectTarget::ObjectMethod {
                object,
                method: method_name,
                called_class: Some(called_class_name.to_string()),
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_method_access_error(
            &declaring_class,
            &method_name,
            visibility,
            context,
            values,
        );
    }
    Ok(EvalClosureObjectTarget::ObjectMethod {
        object,
        method: method_name,
        called_class: Some(called_class_name.to_string()),
        native_class: Some(native_class.to_string()),
        bridge_scope: Some(declaring_class),
    })
}

/// Materializes a first-class static method callable while retaining late-static metadata.
pub(in crate::interpreter) fn eval_static_method_callable_expr(
    class_name: &str,
    method: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    let method = eval_dynamic_member_name(method, context, scope, values)?;
    let target = eval_static_method_callable_target(
        receiver.dispatch_class,
        method,
        Some(receiver.called_class),
        Some(scope),
        context,
        values,
    )?;
    eval_closure_object_expr(target, context, values)
}

/// Materializes a runtime-class static first-class callable as a PHP `Closure` object.
pub(in crate::interpreter) fn eval_dynamic_static_method_callable_expr(
    class_name: &EvalExpr,
    method: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = eval_expr(class_name, context, scope, values)?;
    let class_name = eval_dynamic_class_name(class_name, context, values)?;
    let receiver = resolve_eval_static_method_receiver(&class_name, context)?;
    let method = eval_dynamic_member_name(method, context, scope, values)?;
    let target = eval_static_method_callable_target(
        receiver.dispatch_class,
        method,
        Some(receiver.called_class),
        Some(scope),
        context,
        values,
    )?;
    eval_closure_object_expr(target, context, values)
}

/// Validates and builds the retained target for a static-method first-class callable.
fn eval_static_method_callable_target(
    dispatch_class: String,
    method_name: String,
    called_class: Option<String>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    if let Some((declaring_class, method)) =
        eval_dynamic_static_method_for_call(&dispatch_class, &method_name, context)
    {
        if method.is_abstract() {
            return eval_first_class_abstract_method_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err()
            && !eval_static_magic_callable_for_class(&dispatch_class, context)
        {
            return eval_first_class_method_access_error(
                &declaring_class,
                method.name(),
                method.visibility(),
                context,
                values,
            );
        }
        if !method.is_static() {
            if let Some(object) = eval_static_syntax_instance_receiver(
                &dispatch_class,
                lexical_scope,
                context,
                values,
            )? {
                return Ok(EvalClosureObjectTarget::ObjectMethod {
                    object,
                    method: method_name,
                    called_class,
                    native_class: None,
                    bridge_scope: None,
                });
            }
            return eval_first_class_non_static_method_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        return Ok(EvalClosureObjectTarget::StaticMethod {
            class_name: dispatch_class,
            method: method_name,
            called_class,
            native_class: None,
            bridge_scope: None,
        });
    }
    if context.has_class(&dispatch_class) {
        if let Some(parent) = context.class_native_parent_name(&dispatch_class) {
            return eval_native_static_method_callable_target(
                dispatch_class,
                parent,
                method_name,
                called_class,
                lexical_scope,
                context,
                values,
            );
        }
        if eval_static_magic_callable_for_class(&dispatch_class, context) {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            &dispatch_class,
            &method_name,
            context,
            values,
        );
    }
    if context.has_interface(&dispatch_class)
        || context.has_trait(&dispatch_class)
        || context.has_enum(&dispatch_class)
    {
        if eval_static_magic_callable_for_class(&dispatch_class, context) {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            &dispatch_class,
            &method_name,
            context,
            values,
        );
    }
    eval_native_static_method_callable_target(
        dispatch_class.clone(),
        dispatch_class,
        method_name,
        called_class,
        lexical_scope,
        context,
        values,
    )
}

/// Validates generated/AOT static-method first-class callable metadata.
fn eval_native_static_method_callable_target(
    dispatch_class: String,
    native_class: String,
    method_name: String,
    called_class: Option<String>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(
            &native_class,
            &method_name,
            context,
            values,
        )?
    else {
        if context
            .native_static_method_signature(&native_class, &method_name)
            .is_some()
        {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        if eval_native_static_magic_callable_for_class(&native_class, context, values)?
            || !values.class_exists(&native_class)?
        {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            &dispatch_class,
            &method_name,
            context,
            values,
        );
    };
    if is_abstract {
        return eval_first_class_abstract_method_error(
            &declaring_class,
            &method_name,
            context,
            values,
        );
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        if eval_native_static_magic_callable_for_class(&native_class, context, values)? {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_method_access_error(
            &declaring_class,
            &method_name,
            visibility,
            context,
            values,
        );
    }
    if !is_static {
        if let Some(object) = eval_static_syntax_instance_receiver(
            &dispatch_class,
            lexical_scope,
            context,
            values,
        )? {
            return Ok(EvalClosureObjectTarget::ObjectMethod {
                object,
                method: method_name,
                called_class,
                native_class: Some(native_class),
                bridge_scope: Some(declaring_class),
            });
        }
        return eval_first_class_non_static_method_error(
            &declaring_class,
            &method_name,
            context,
            values,
        );
    }
    Ok(EvalClosureObjectTarget::StaticMethod {
        class_name: dispatch_class,
        method: method_name,
        called_class,
        native_class: Some(native_class),
        bridge_scope: Some(declaring_class),
    })
}
