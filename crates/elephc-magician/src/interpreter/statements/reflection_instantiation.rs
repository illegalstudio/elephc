//! Purpose:
//! Implements Reflection-driven class and attribute instantiation plus access checks.
//!
//! Called from:
//! - Method dispatch for ReflectionClass and ReflectionAttribute owners.
//!
//! Key details:
//! - Constructor visibility, without-constructor allocation, and attribute argument materialization align with PHP.

use super::*;

/// Returns the runtime-visible class name for a non-eval object receiver.
pub(in crate::interpreter) fn runtime_object_class_name(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Instantiates the class named by a materialized eval `ReflectionClass` object.
pub(super) fn eval_reflection_class_new_instance_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let direct_new_instance = method_name.eq_ignore_ascii_case("newInstance");
    let constructor_args = if direct_new_instance {
        eval_reflection_constructor_by_value_args(evaluated_args)
    } else if method_name.eq_ignore_ascii_case("newInstanceArgs") {
        eval_reflection_class_new_instance_args(evaluated_args, context, values)?
    } else {
        return Ok(None);
    };
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if let Some(message) =
        eval_reflection_eval_instantiation_error_message(&reflected_name, context)
    {
        return eval_throw_error(&message, context, values);
    }
    if let Some(class) = context.class(&reflected_name).cloned() {
        if let Some((_, constructor)) = context.class_method(class.name(), "__construct") {
            if constructor.visibility() != EvalVisibility::Public {
                return eval_throw_reflection_exception(
                    &format!(
                        "Access to non-public constructor of class {}",
                        class.name()
                    ),
                    context,
                    values,
                );
            }
        }
        return eval_reflection_public_constructor_scope(context, values, |context, values| {
            let constructor_name =
                format!("{}::__construct", class.name().trim_start_matches('\\'));
            let by_ref_mode = EvalByRefBindingMode::WarnByValue {
                callable_name: &constructor_name,
            };
            let mut scope = ElephcEvalScope::new();
            eval_dynamic_class_new_object_with_ref_mode(
                &class,
                constructor_args,
                by_ref_mode,
                context,
                &mut scope,
                values,
            )
            .map(Some)
        });
    }
    let class_name = context
        .resolve_class_name(&reflected_name)
        .unwrap_or(reflected_name);
    if let Some(error) = eval_reflection_aot_class_public_instantiation_error(&class_name, values)?
    {
        return eval_throw_reflection_instantiation_error(error, context, values);
    }
    eval_reflection_public_constructor_scope(context, values, |context, values| {
        let constructor_name = format!("{}::__construct", class_name.trim_start_matches('\\'));
        let by_ref_mode = EvalByRefBindingMode::WarnByValue {
            callable_name: &constructor_name,
        };
        let instance = values.new_object(&class_name)?;
        eval_native_constructor_with_evaluated_args_and_ref_mode(
            &class_name,
            instance,
            constructor_args,
            by_ref_mode,
            context,
            values,
        )?;
        Ok(Some(instance))
    })
}

/// Removes caller writeback targets for ReflectionClass::newInstance() by-value forwarding.
pub(super) fn eval_reflection_constructor_by_value_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Vec<EvaluatedCallArg> {
    evaluated_args
        .into_iter()
        .map(|arg| EvaluatedCallArg {
            name: arg.name,
            value: arg.value,
            ref_target: None,
        })
        .collect()
}

/// Expands the single `ReflectionClass::newInstanceArgs()` array argument.
pub(super) fn eval_reflection_class_new_instance_args(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("args")], evaluated_args)?;
    eval_array_call_arg_values(args[0], context, values)
}

/// Runs ReflectionClass construction with only public constructor visibility.
pub(super) fn eval_reflection_public_constructor_scope<T, V: RuntimeValueOps>(
    context: &mut ElephcEvalContext,
    values: &mut V,
    action: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    context.push_class_scope(String::new());
    let result = action(context, values);
    context.pop_class_scope();
    result
}

/// Allocates the class named by a materialized eval `ReflectionClass` without running `__construct()`.
pub(super) fn eval_reflection_class_new_instance_without_constructor_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("newInstanceWithoutConstructor") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if let Some(message) =
        eval_reflection_eval_instantiation_error_message(&reflected_name, context)
    {
        return eval_throw_error(&message, context, values);
    }
    if let Some(class) = context.class(&reflected_name).cloned() {
        let mut scope = ElephcEvalScope::new();
        return eval_dynamic_class_allocate_object(&class, context, &mut scope, values).map(Some);
    }
    if context.has_interface(&reflected_name)
        || context.has_trait(&reflected_name)
        || context.has_enum(&reflected_name)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class_name = context
        .resolve_class_name(&reflected_name)
        .unwrap_or(reflected_name);
    if let Some(message) =
        eval_reflection_aot_class_without_constructor_error(&class_name, values)?
    {
        return eval_throw_error(&message, context, values);
    }
    values.new_object(&class_name).map(Some)
}

/// Builds PHP's reflection instantiation error for eval non-instantiable class-likes.
pub(super) fn eval_reflection_eval_instantiation_error_message(
    reflected_name: &str,
    context: &ElephcEvalContext,
) -> Option<String> {
    if let Some(class) = context.class(reflected_name) {
        if class.is_abstract() {
            return Some(format!("Cannot instantiate abstract class {}", class.name()));
        }
        if let Some(enum_decl) = context.enum_decl(class.name()) {
            return Some(format!("Cannot instantiate enum {}", enum_decl.name()));
        }
        return None;
    }
    if let Some(interface) = context.interface(reflected_name) {
        return Some(format!("Cannot instantiate interface {}", interface.name()));
    }
    if let Some(trait_decl) = context.trait_decl(reflected_name) {
        return Some(format!("Cannot instantiate trait {}", trait_decl.name()));
    }
    context
        .enum_decl(reflected_name)
        .map(|enum_decl| format!("Cannot instantiate enum {}", enum_decl.name()))
}

/// Instantiates an attribute class for `ReflectionAttribute::newInstance()`.
pub(super) fn eval_reflection_attribute_new_instance_result(
    attribute: &EvalAttribute,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = eval_reflection_attribute_evaluated_args(attribute, values)?;
    if let Some(class) = context.class(attribute.name()).cloned() {
        let mut scope = ElephcEvalScope::new();
        return eval_dynamic_class_new_object(&class, args, context, &mut scope, values);
    }
    let class_name = context
        .resolve_class_name(attribute.name())
        .unwrap_or_else(|| attribute.name().trim_start_matches('\\').to_string());
    if !values.class_exists(&class_name)? {
        return values.null();
    }
    let object = values.new_object(&class_name)?;
    if let Err(err) = eval_native_constructor_with_evaluated_args(
        &class_name,
        object,
        args,
        context,
        values,
    ) {
        let _ = values.release(object);
        return Err(err);
    }
    Ok(object)
}

/// Materializes eval attribute literal arguments as evaluated constructor args.
pub(super) fn eval_reflection_attribute_evaluated_args(
    attribute: &EvalAttribute,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let Some(args) = attribute.args() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    args.iter()
        .map(|arg| {
            Ok(EvaluatedCallArg {
                name: arg.name().map(str::to_string),
                value: eval_reflection_attribute_arg_value(arg.value(), values)?,
                ref_target: None,
            })
        })
        .collect()
}

/// Materializes one eval attribute literal as a constructor argument cell.
pub(super) fn eval_reflection_attribute_arg_value(
    arg: &EvalAttributeArg,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match arg {
        EvalAttributeArg::String(value) => values.string(value),
        EvalAttributeArg::Int(value) => values.int(*value),
        EvalAttributeArg::Float(bits) => values.float(f64::from_bits(*bits)),
        EvalAttributeArg::Bool(value) => values.bool_value(*value),
        EvalAttributeArg::Null => values.null(),
        EvalAttributeArg::Array(elements) => {
            eval_reflection_attribute_array_arg_value(elements, values)
        }
        EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
            eval_reflection_attribute_arg_value(value, values)
        }
    }
}

/// Materializes one retained attribute array literal for constructor calls.
pub(super) fn eval_reflection_attribute_array_arg_value(
    elements: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = if elements
        .iter()
        .any(|element| element.name().is_some() || element.int_key().is_some())
    {
        values.assoc_new(elements.len())?
    } else {
        values.array_new(elements.len())?
    };
    for (index, element) in elements.iter().enumerate() {
        let key = match element.name() {
            Some(name) => values.string(name)?,
            None => values.int(element.int_key().unwrap_or(index as i64))?,
        };
        let value = eval_reflection_attribute_arg_value(element.value(), values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Resolves the method metadata visible from the current class scope.
pub(in crate::interpreter) fn eval_dynamic_method_for_call(
    object_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, method)) =
                context.class_own_method(current_class, method_name)
            {
                if method.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, method));
                }
            }
        }
    }
    context.class_method(object_class_name, method_name)
}

/// Returns whether the current eval class scope can access one declared member.
pub(in crate::interpreter) fn validate_eval_member_access(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if visibility == EvalVisibility::Public {
        return Ok(());
    }
    let Some(current_class) = context.current_class_scope() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    match visibility {
        EvalVisibility::Public => Ok(()),
        EvalVisibility::Private => same_eval_class_name(current_class, declaring_class)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal),
        EvalVisibility::Protected => {
            eval_classes_are_related(current_class, declaring_class, context)
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)
        }
    }
}

/// Returns true when two PHP class names refer to the same eval class.
pub(super) fn same_eval_class_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns true when two eval or generated classes are in the same inheritance family.
pub(super) fn eval_classes_are_related(left: &str, right: &str, context: &ElephcEvalContext) -> bool {
    same_eval_class_name(left, right)
        || context.class_is_a(left, right, false)
        || context.class_is_a(right, left, false)
        || native_class_is_a(left, right, context)
        || native_class_is_a(right, left, context)
}

/// Returns true when generated AOT parent metadata proves one class extends another.
pub(super) fn native_class_is_a(class_name: &str, target: &str, context: &ElephcEvalContext) -> bool {
    let mut current = class_name.trim_start_matches('\\').to_string();
    let target = target.trim_start_matches('\\');
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return false;
        }
        if same_eval_class_name(&current, target) {
            return true;
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return false;
        };
        current = parent.to_string();
    }
}
