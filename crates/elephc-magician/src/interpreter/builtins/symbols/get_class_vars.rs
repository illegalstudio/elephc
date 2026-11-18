//! Purpose:
//! Eval registry entry and implementation for `get_class_vars`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval-declared defaults are materialized in the declaring class scope.
//! - Generated/AOT defaults use native callable default metadata when present.

eval_builtin! {
    contract: "get_class_vars",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::super::collection_builder::EvalArrayBuilder;
use super::super::{
    eval_class_relation_name_exists, eval_resolved_class_metadata_name,
    eval_runtime_property_access_metadata, eval_runtime_string_array_to_vec,
};
use std::collections::HashSet;

/// Dispatches direct eval calls for the `get_class_vars` symbol builtin.
pub(in crate::interpreter) fn eval_get_class_vars_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_class_vars(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `get_class_vars` symbol builtin.
pub(in crate::interpreter) fn eval_get_class_vars_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_get_class_vars_result(evaluated_args, context, values)
}

/// Evaluates `get_class_vars()` from eval expressions.
pub(in crate::interpreter) fn eval_builtin_get_class_vars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    with_eval_operands(&[target], context, scope, values, |args, context, _, values| {
        eval_get_class_vars_result(args, context, values)
    })
}

/// Evaluates materialized `get_class_vars()` arguments.
pub(in crate::interpreter) fn eval_get_class_vars_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [target] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let class_name = eval_resolved_class_metadata_name(*target, context, values)?;
    if context.has_class(&class_name) || context.has_enum(&class_name) {
        // php-src initializes the class, then copies each default it returns, evaluating every
        // visible static default again on each call; the instance defaults were evaluated in
        // place by the initialization.
        eval_raise_class_initialization_deprecations(&class_name, context, values)?;
        eval_raise_visible_static_default_deprecations(&class_name, context, values)?;
        context.begin_deprecation_silence();
        let result = eval_dynamic_class_vars_result(&class_name, context, values);
        context.end_deprecation_silence();
        return result;
    }
    if context.has_trait(&class_name) {
        eval_raise_trait_vars_deprecations(&class_name, context, values)?;
        context.begin_deprecation_silence();
        let result = eval_dynamic_trait_vars_result(&class_name, context, values);
        context.end_deprecation_silence();
        return result;
    }
    if context.has_interface(&class_name) {
        // No properties, but php-src still evaluates the interface's constant table.
        eval_class_constant_table_evaluation(&class_name, context, values)?;
        return values.assoc_new(0);
    }
    if eval_class_relation_name_exists(&class_name, context, values)? {
        return eval_runtime_class_vars_result(&class_name, context, values);
    }
    eval_throw_type_error(
        &format!("get_class_vars(): Argument #1 ($class) must be a valid class name, {class_name} given"),
        context, values,
    )
}

/// Builds `get_class_vars()` for an eval-declared class or enum.
fn eval_dynamic_class_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut entries = match context.class_native_parent_name(class_name) {
        Some(parent) => eval_runtime_class_var_entries(&parent, context, values)?,
        None => Vec::new(),
    };
    for class in context.class_chain(class_name) {
        for property in class.properties() {
            if property.is_virtual()
                || validate_eval_member_access(class.name(), property.visibility(), context).is_err()
            {
                continue;
            }
            let entry = ClassVarEntry {
                name: property.name().to_string(),
                is_static: property.is_static(),
                default: ClassVarDefault::Eval(class.name().to_string(), property.clone()),
            };
            if let Some(slot) = entries.iter_mut().find(|slot| slot.name == entry.name) {
                *slot = entry;
            } else {
                entries.push(entry);
            }
        }
    }
    let mut result = EvalArrayBuilder::assoc(values, 0)?;
    if let Some(enum_decl) = context.enum_decl(class_name) {
        result.string("name", |values| values.null())?;
        if enum_decl.backing_type().is_some() {
            result.string("value", |values| values.null())?;
        }
    }
    append_class_var_entries(&mut result, entries, context)?;
    Ok(result.finish())
}

/// Stores one visible property and its default's lexical owner before result allocation.
struct ClassVarEntry {
    name: String,
    is_static: bool,
    default: ClassVarDefault,
}

/// Keeps native defaults distinct from eval expressions until their declaration scope is entered.
enum ClassVarDefault {
    Eval(String, EvalClassProperty),
    Native { runtime_class: String, declaring_class: String },
}

/// Emits instance defaults before static defaults, retaining inherited declaration order.
fn append_class_var_entries<V: RuntimeValueOps>(
    result: &mut EvalArrayBuilder<'_, V>,
    entries: Vec<ClassVarEntry>,
    context: &mut ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for entry in entries.iter().filter(|entry| !entry.is_static)
        .chain(entries.iter().filter(|entry| entry.is_static))
    {
        result.string(&entry.name, |values| match &entry.default {
            ClassVarDefault::Eval(owner, property) => {
                eval_class_vars_property_default_value(owner, property, context, values)
            }
            ClassVarDefault::Native { runtime_class, declaring_class } => {
                eval_runtime_class_var_default_value(
                    runtime_class, declaring_class, &entry.name, context, values,
                )
            }
        })?;
    }
    Ok(())
}

/// Raises what `get_class_vars()` reports for an eval-declared trait: php-src initializes it
/// like a class without ancestors (its instance defaults, then its static ones, once), then
/// evaluates every visible static default again on each call.
fn eval_raise_trait_vars_deprecations(
    trait_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.deprecations_silenced() {
        return Ok(());
    }
    let Some(trait_decl) = context.trait_decl(trait_name) else { return Ok(()) };
    let name = trait_decl.name().to_string();
    let properties = trait_decl.properties().to_vec();
    let constants: Vec<String> = trait_decl.constants().iter().map(|constant| constant.name().to_string()).collect();
    let instance = properties.iter().filter(|property| !property.is_static());
    let statics = properties.iter().filter(|property| property.is_static());
    if context.begin_class_initialization(&name) {
        // The trait's own constants first, held by the trait itself, then its instance and
        // static defaults. An initializer that throws leaves the trait uninitialized, so the
        // next use initializes it again, as php-src does.
        let initialized = (|| -> Result<(), EvalStatus> {
            for constant in &constants {
                eval_class_constant_first_evaluation(&name, constant, context, values)?;
            }
            for property in instance.chain(statics.clone()) {
                eval_trait_default_evaluation(&name, property, context, values)?;
            }
            Ok(())
        })();
        if initialized.is_err() {
            context.abort_class_initialization(&name);
        }
        initialized?;
    }
    let replayed: Vec<&EvalClassProperty> = statics
        .filter(|property| validate_eval_member_access(&name, property.visibility(), context).is_ok())
        .collect();
    for property in replayed {
        eval_trait_default_evaluation(&name, property, context, values)?;
    }
    Ok(())
}

/// Evaluates trait `name`'s default for `property`, if it has one, and drops the value.
fn eval_trait_default_evaluation(
    name: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if property.default().is_none() || property.is_virtual() {
        return Ok(());
    }
    let value = eval_class_vars_property_default_value(name, property, context, values)?;
    values.release(value)
}

/// Builds `get_class_vars()` for an eval-declared trait.
fn eval_dynamic_trait_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(trait_decl) = context.trait_decl(class_name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let trait_name = trait_decl.name().to_string();
    let properties = trait_decl.properties().to_vec();
    let mut result = EvalArrayBuilder::assoc(values, properties.len())?;
    let mut emitted_keys = HashSet::new();
    for property in properties.iter().filter(|property| !property.is_static())
        .chain(properties.iter().filter(|property| property.is_static()))
    {
        if property.is_virtual() || emitted_keys.contains(property.name())
            || validate_eval_member_access(&trait_name, property.visibility(), context).is_err()
        {
            continue;
        }
        result.string(property.name(), |values| {
            eval_class_vars_property_default_value(&trait_name, property, context, values)
        })?;
        emitted_keys.insert(property.name().to_string());
    }
    Ok(result.finish())
}

/// Builds `get_class_vars()` data for generated/AOT class metadata.
fn eval_runtime_class_vars_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let entries = eval_runtime_class_var_entries(class_name, context, values)?;
    let mut result = EvalArrayBuilder::assoc(values, entries.len())?;
    append_class_var_entries(&mut result, entries, context)?;
    Ok(result.finish())
}

/// Collects native property metadata for direct introspection or an eval subclass's inherited slots.
fn eval_runtime_class_var_entries(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<ClassVarEntry>, EvalStatus> {
    let property_names = values.reflection_property_names(class_name)?;
    let declared_names = eval_runtime_string_array_to_vec(property_names, values);
    values.release(property_names)?;
    let declared_names = declared_names?;
    let mut entries = Vec::new();
    let mut emitted_keys = HashSet::new();
    for property_name in declared_names {
        if emitted_keys.contains(&property_name) {
            continue;
        }
        let Some((declaring_class, visibility, is_static)) =
            eval_runtime_property_access_metadata(class_name, &property_name, values)?
        else {
            continue;
        };
        if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
            continue;
        }
        if values.reflection_property_flags(class_name, &property_name)?
            .is_some_and(|flags| flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL != 0)
        {
            continue;
        }
        entries.push(ClassVarEntry {
            name: property_name.clone(), is_static,
            default: ClassVarDefault::Native { runtime_class: class_name.to_string(), declaring_class },
        });
        emitted_keys.insert(property_name);
    }
    Ok(entries)
}

/// Materializes one eval-declared property default for `get_class_vars()`.
fn eval_class_vars_property_default_value(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(default) = property.default() else {
        return values.null();
    };
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    context.push_class_like_member_magic_scope(declaring_class, property.trait_origin());
    let result = eval_method_parameter_default(default, context, values);
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
}

/// Materializes one generated/AOT property default for `get_class_vars()`.
fn eval_runtime_class_var_default_value(
    runtime_class: &str,
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(default) = context
        .native_property_default(declaring_class, property_name)
        .or_else(|| context.native_property_default(runtime_class, property_name))
    {
        return materialize_native_callable_default(&default, context, values);
    }
    values.null()
}
