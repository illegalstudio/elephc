//! Purpose:
//! Eval registry entry and implementation for `class_attribute_names`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Shared class-attribute metadata logic for `class_attribute_args()` and
//!   `class_get_attributes()` lives here.
//! - Constant-expression arguments (`EvalAttributeArg::Expr`) are evaluated here, when an
//!   attribute is read, and continue as literals.

eval_builtin! {
    contract: "class_attribute_names",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::super::eval_class_metadata_name;

/// Dispatches direct eval calls for the `class_attribute_names` symbol builtin.
pub(in crate::interpreter) fn eval_class_attribute_names_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_class_attribute_metadata("class_attribute_names", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_attribute_names` symbol builtin.
pub(in crate::interpreter) fn eval_class_attribute_names_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_attribute_metadata_result("class_attribute_names", evaluated_args, context, values)
}

/// Evaluates class attribute metadata helpers.
pub(in crate::interpreter) fn eval_builtin_class_attribute_metadata(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = match (name, args) {
        ("class_attribute_names" | "class_get_attributes", [class_name]) => {
            vec![eval_expr(class_name, context, scope, values)?]
        }
        ("class_attribute_args", [class_name, attribute_name]) => vec![
            eval_expr(class_name, context, scope, values)?,
            eval_expr(attribute_name, context, scope, values)?,
        ],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_class_attribute_metadata_result(name, &evaluated_args, context, values)
}

/// Evaluates materialized class attribute metadata arguments.
pub(in crate::interpreter) fn eval_class_attribute_metadata_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match (name, evaluated_args) {
        ("class_attribute_names", [class_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let attributes = eval_class_like_attribute_metadata(context, &class_name);
            eval_class_attribute_names_result(&attributes, values)
        }
        ("class_get_attributes", [class_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let attributes = eval_class_like_attribute_metadata(context, &class_name);
            eval_reflection_attribute_array_result(
                &attributes,
                EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS,
                context,
                values,
            )
        }
        ("class_attribute_args", [class_name, attribute_name]) => {
            let class_name = eval_class_metadata_name(*class_name, values)?;
            let attribute_name = eval_class_metadata_name(*attribute_name, values)?;
            let attributes = eval_class_like_attribute_metadata(context, &class_name);
            let Some(attribute) = attributes
                .iter()
                .find(|attribute| eval_attribute_name_matches(attribute.name(), &attribute_name))
            else {
                return values.array_new(0);
            };
            let Some(args) = attribute.args() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let args = eval_resolve_attribute_args(args, context, values)?;
            eval_class_attribute_args_result(&args, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns class-like attributes for a dynamic eval class, interface, trait, or enum.
fn eval_class_like_attributes<'a>(
    context: &'a ElephcEvalContext,
    name: &str,
) -> Option<&'a [EvalAttribute]> {
    if let Some(class) = context.class(name) {
        return Some(class.attributes());
    }
    if let Some(interface) = context.interface(name) {
        return Some(interface.attributes());
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some(trait_decl.attributes());
    }
    context.enum_decl(name).map(EvalEnum::attributes)
}

/// Returns class-like attributes for eval declarations or generated AOT metadata.
fn eval_class_like_attribute_metadata(
    context: &ElephcEvalContext,
    name: &str,
) -> Vec<EvalAttribute> {
    if let Some(attributes) = eval_class_like_attributes(context, name) {
        return attributes.to_vec();
    }
    context.native_class_attributes(name)
}

/// Stores one element into `array` and releases the temporary key and value owners.
///
/// `RuntimeValueOps::array_set` borrows its key and value (storage takes its own reference), so a
/// caller that built them for this store owns one reference of each. Not giving them back leaked
/// two cells per element and, for a materialized `ReflectionAttribute`, the object with its
/// argument array -- 24 blocks per `new ReflectionFunction($closure)` parameter walk.
fn eval_array_set_owned(
    array: RuntimeCellHandle,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = values.array_set(array, key, value);
    let key_release = values.release(key);
    let value_release = values.release(value);
    match result {
        Ok(array) => {
            key_release?;
            value_release?;
            Ok(array)
        }
        Err(status) => {
            let _ = values.release(array);
            Err(status)
        }
    }
}

/// Builds the indexed string array returned by `class_attribute_names()`.
fn eval_class_attribute_names_result(
    attributes: &[EvalAttribute],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(attributes.len())?;
    for (index, attribute) in attributes.iter().enumerate() {
        let key = values.int(index as i64)?;
        let value = values.string(attribute.name())?;
        result = eval_array_set_owned(result, key, value, values)?;
    }
    Ok(result)
}

/// Builds an indexed `ReflectionAttribute` array from eval-retained attribute metadata.
pub(in crate::interpreter) fn eval_reflection_attribute_array_result(
    attributes: &[EvalAttribute],
    target: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(attributes.len())?;
    for (index, attribute) in attributes.iter().enumerate() {
        let Some(args) = attribute.args() else {
            crate::errors::note_eval_runtime_failure(
                format!("attribute #[{}] carries arguments reflection cannot read", attribute.name()),
                "",
                None,
            );
            return Err(EvalStatus::RuntimeFatal);
        };
        // Constant expressions are folded to their values first, the way php evaluates an
        // attribute's arguments; everything below -- the argument array, `__toString()`, and the
        // metadata a later `newInstance()` reads -- then sees literals only.
        let resolved_args = eval_resolve_attribute_args(args, context, values)?;
        let attribute = EvalAttribute::new(attribute.name(), Some(resolved_args));
        let args = attribute.args().unwrap_or(&[]);
        let key = values.int(index as i64)?;
        let args = eval_class_attribute_args_result(args, values)?;
        let repeated = eval_attribute_is_repeated(attributes, attribute.name());
        // Rendered HERE, not in the emitted helper: PHP's `__toString()` spells out every literal
        // argument, and only the interpreter holds them for a runtime-declared class.
        let rendered = crate::eval_ir::render_reflection_attribute_string(
            attribute.name(),
            attribute.args().unwrap_or(&[]),
        );
        let reflection_attribute =
            values.reflection_attribute_new(attribute.name(), args, target, repeated, &rendered)?;
        let identity = values.object_identity(reflection_attribute)?;
        context.register_eval_reflection_attribute(identity, attribute.clone(), target, repeated);
        values.release(args)?;
        result = eval_array_set_owned(result, key, reflection_attribute, values)?;
    }
    Ok(result)
}

/// Folds every constant-expression argument of an attribute to the literal it evaluates to.
///
/// php evaluates an attribute's arguments when reflection reads them, and a class constant
/// becomes its value (`Requirement::POSITIVE_INT` reads as `'[1-9][0-9]*'`, also in
/// `__toString()`). An argument that evaluates to an object (an enum case) has no literal form
/// here and stays a refusal.
pub(in crate::interpreter) fn eval_resolve_attribute_args(
    args: &[EvalAttributeArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAttributeArg>, EvalStatus> {
    args.iter()
        .map(|arg| eval_resolve_attribute_arg(arg, context, values))
        .collect()
}

/// Folds one attribute argument, recursing through names, keys and array elements.
fn eval_resolve_attribute_arg(
    arg: &EvalAttributeArg,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalAttributeArg, EvalStatus> {
    Ok(match arg {
        EvalAttributeArg::Expr(expr) => {
            let mut scope = ElephcEvalScope::new();
            // php evaluates an attribute argument inside the class that carries it, so a
            // `self::HELP` naming a private constant reads fine (Symfony's
            // `#[AsCommand(help: self::HELP)]`). The compiled side already substituted the
            // declaring class for `self`, so the named class is the evaluation scope.
            let scope_class = match expr.as_ref() {
                crate::eval_ir::EvalExpr::ClassConstantFetch { class_name, .. } => {
                    Some(class_name.trim_start_matches('\\').to_string())
                }
                _ => None,
            };
            if let Some(class) = &scope_class {
                context.push_class_scope(class.clone());
            }
            let evaluated = eval_expr(expr, context, &mut scope, values);
            if scope_class.is_some() {
                context.pop_class_scope();
            }
            let value = evaluated.inspect_err(|status| {
                crate::errors::note_eval_runtime_failure(
                    format!("could not evaluate an attribute argument {expr:?} ({status:?})"),
                    "",
                    None,
                );
            })?;
            let literal = eval_attribute_arg_from_value(value, values);
            let released = release_expr_result(value, context, values);
            let literal = literal?;
            released?;
            literal
        }
        EvalAttributeArg::Named { name, value } => EvalAttributeArg::Named {
            name: name.clone(),
            value: Box::new(eval_resolve_attribute_arg(value, context, values)?),
        },
        EvalAttributeArg::IntKeyed { key, value } => EvalAttributeArg::IntKeyed {
            key: *key,
            value: Box::new(eval_resolve_attribute_arg(value, context, values)?),
        },
        EvalAttributeArg::Array(elements) => EvalAttributeArg::Array(
            elements
                .iter()
                .map(|element| eval_resolve_attribute_arg(element, context, values))
                .collect::<Result<_, _>>()?,
        ),
        literal => literal.clone(),
    })
}

/// Converts an evaluated runtime value back into literal attribute metadata.
///
/// Borrows `value`; every key and element read out of an array is released once converted.
fn eval_attribute_arg_from_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalAttributeArg, EvalStatus> {
    Ok(match values.type_tag(value)? {
        EVAL_TAG_NULL => EvalAttributeArg::Null,
        EVAL_TAG_BOOL => EvalAttributeArg::Bool(values.truthy(value)?),
        EVAL_TAG_INT => EvalAttributeArg::Int(eval_int_value(value, values)?),
        EVAL_TAG_FLOAT => EvalAttributeArg::Float(eval_float_value(value, values)?.to_bits()),
        EVAL_TAG_STRING => match String::from_utf8(values.string_bytes(value)?) {
            Ok(text) => EvalAttributeArg::String(text),
            Err(error) => EvalAttributeArg::Bytes(error.into_bytes()),
        },
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            let len = values.array_len(value)?;
            let mut elements = Vec::with_capacity(len);
            for position in 0..len {
                let key = values.array_iter_key(value, position)?;
                let element = match values.array_iter_value(value, position) {
                    Ok(element) => element,
                    Err(status) => {
                        let _ = values.release(key);
                        return Err(status);
                    }
                };
                let converted = eval_attribute_array_element_from_cells(
                    key, element, position, values,
                );
                let key_released = values.release(key);
                let element_released = values.release(element);
                let converted = converted?;
                key_released?;
                element_released?;
                elements.push(converted);
            }
            EvalAttributeArg::Array(elements)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    })
}

/// Converts one borrowed array entry into an attribute array element, keeping its key: a string
/// key becomes `Named`, an integer key other than its position becomes `IntKeyed`.
fn eval_attribute_array_element_from_cells(
    key: RuntimeCellHandle,
    element: RuntimeCellHandle,
    position: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalAttributeArg, EvalStatus> {
    let element = eval_attribute_arg_from_value(element, values)?;
    if values.type_tag(key)? == EVAL_TAG_STRING {
        return Ok(EvalAttributeArg::Named {
            name: String::from_utf8_lossy(&values.string_bytes(key)?).into_owned(),
            value: Box::new(element),
        });
    }
    let key = eval_int_value(key, values)?;
    Ok(if key == position as i64 {
        element
    } else {
        EvalAttributeArg::IntKeyed {
            key,
            value: Box::new(element),
        }
    })
}

/// Returns true when an attribute name appears more than once on the same owner.
fn eval_attribute_is_repeated(attributes: &[EvalAttribute], name: &str) -> bool {
    attributes
        .iter()
        .filter(|attribute| eval_attribute_name_matches(attribute.name(), name))
        .nth(1)
        .is_some()
}

/// Builds the mixed PHP array returned by `class_attribute_args()`.
pub(in crate::interpreter) fn eval_class_attribute_args_result(
    args: &[EvalAttributeArg],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = if args.iter().any(|arg| arg.name().is_some()) {
        values.assoc_new(args.len())?
    } else {
        values.array_new(args.len())?
    };
    for (index, arg) in args.iter().enumerate() {
        let key = match arg.name() {
            Some(name) => values.string(name)?,
            None => values.int(index as i64)?,
        };
        let value = eval_class_attribute_arg_value(arg.value(), values)?;
        result = eval_array_set_owned(result, key, value, values)?;
    }
    Ok(result)
}

/// Materializes one retained eval attribute argument as a runtime cell.
fn eval_class_attribute_arg_value(
    arg: &EvalAttributeArg,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match arg {
        EvalAttributeArg::String(value) => values.string_literal(value),
        EvalAttributeArg::Bytes(value) => values.string_literal_bytes(value),
        EvalAttributeArg::Int(value) => values.int(*value),
        EvalAttributeArg::Float(bits) => values.float(f64::from_bits(*bits)),
        EvalAttributeArg::Bool(value) => values.bool_value(*value),
        EvalAttributeArg::Null => values.null(),
        EvalAttributeArg::Array(elements) => eval_class_attribute_array_arg_value(elements, values),
        EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
            eval_class_attribute_arg_value(value, values)
        }
        // An expression needs the interpreter to evaluate it; every caller folds it first.
        EvalAttributeArg::Expr(_) => Err(EvalStatus::RuntimeFatal),
    }
}

/// Materializes one retained attribute array literal as a runtime array cell.
fn eval_class_attribute_array_arg_value(
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
        let value = eval_class_attribute_arg_value(element.value(), values)?;
        result = eval_array_set_owned(result, key, value, values)?;
    }
    Ok(result)
}

/// Returns whether a query names the same PHP attribute class case-insensitively.
fn eval_attribute_name_matches(attribute_name: &str, query: &str) -> bool {
    attribute_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(query.trim_start_matches('\\'))
}
