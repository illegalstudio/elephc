//! Purpose:
//! Implements eval-side debug output builtins such as `print_r()` and `var_dump()`.
//!
//! Called from:
//! - `crate::interpreter::eval_positional_expr_call()` for debug-output builtin dispatch.
//!
//! Key details:
//! - Output formatting walks runtime arrays, scalars, object names, and public or
//!   eval-declared object properties through `RuntimeValueOps` and eval context metadata.
//! - Eval property aliases are rendered as references when dumping eval-declared objects.
//! - `print_r($value, true)` returns captured output instead of echoing it.

use std::collections::HashSet;

use super::*;

/// Property visibility rendered by `var_dump()` and `print_r()` object output.
#[derive(Clone)]
struct EvalDebugPropertyVisibility {
    kind: EvalDebugPropertyVisibilityKind,
}

/// Concrete PHP visibility shape for one object property key.
#[derive(Clone)]
enum EvalDebugPropertyVisibilityKind {
    Public,
    Protected,
    Private(String),
}

/// Object property entry collected before rendering object headers.
#[derive(Clone)]
struct EvalDebugObjectProperty {
    name: String,
    visibility: EvalDebugPropertyVisibility,
    value: RuntimeCellHandle,
    is_reference: bool,
}

/// Evaluates PHP `print_r()` over one value and an optional return flag.
pub(super) fn eval_builtin_print_r(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let value = eval_expr(&args[0], context, scope, values)?;
    let return_output = match args.get(1) {
        Some(arg) => {
            let flag = eval_expr(arg, context, scope, values)?;
            values.truthy(flag)?
        }
        None => false,
    };
    eval_print_r_value_result(value, return_output, context, values)
}

/// Evaluates already materialized `print_r()` arguments.
pub(in crate::interpreter) fn eval_print_r_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let return_output = match args.get(1) {
        Some(flag) => values.truthy(*flag)?,
        None => false,
    };
    eval_print_r_value_result(args[0], return_output, context, values)
}

/// Renders, echoes, or returns one `print_r()` output string.
fn eval_print_r_value_result(
    value: RuntimeCellHandle,
    return_output: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut output = Vec::new();
    let mut arrays_seen = Vec::new();
    let mut objects_seen = Vec::new();
    eval_print_r_append_value(
        value,
        context,
        values,
        0,
        &mut arrays_seen,
        &mut objects_seen,
        &mut output,
    )?;
    let output = values.string_bytes_value(&output)?;
    if return_output {
        Ok(output)
    } else {
        values.echo(output)?;
        values.bool_value(true)
    }
}

/// Evaluates PHP `var_dump()` over one or more eval expressions and returns null.
pub(super) fn eval_builtin_var_dump(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_var_dump_result(&evaluated_args, context, values)
}

/// Emits already materialized values using PHP-style `var_dump()` debug formatting.
pub(in crate::interpreter) fn eval_var_dump_result(
    values_to_dump: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values_to_dump.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut output = Vec::new();
    let mut arrays_seen = Vec::new();
    let mut objects_seen = Vec::new();
    for value in values_to_dump {
        eval_var_dump_append_value(
            *value,
            context,
            values,
            0,
            false,
            &mut arrays_seen,
            &mut objects_seen,
            &mut output,
        )?;
    }
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.null()
}

/// Appends one value and its nested entries to a `var_dump()` byte buffer.
fn eval_var_dump_append_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    arrays_seen: &mut Vec<usize>,
    objects_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => {
            eval_var_dump_append_scalar(b"int", value, values, depth, is_reference, output)
        }
        EVAL_TAG_STRING => eval_var_dump_append_string(value, values, depth, is_reference, output),
        EVAL_TAG_FLOAT => {
            eval_var_dump_append_scalar(b"float", value, values, depth, is_reference, output)
        }
        EVAL_TAG_BOOL => eval_var_dump_append_bool(value, values, depth, is_reference, output),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => eval_var_dump_append_array(
            value,
            context,
            values,
            depth,
            is_reference,
            arrays_seen,
            objects_seen,
            output,
        ),
        EVAL_TAG_OBJECT => eval_var_dump_append_object(
            value,
            context,
            values,
            depth,
            is_reference,
            arrays_seen,
            objects_seen,
            output,
        ),
        EVAL_TAG_NULL => {
            eval_var_dump_append_prefix(depth, is_reference, output);
            output.extend_from_slice(b"NULL\n");
            Ok(())
        }
        EVAL_TAG_RESOURCE => {
            eval_var_dump_append_prefix(depth, is_reference, output);
            output.extend_from_slice(b"resource(0) of type (stream)\n");
            Ok(())
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends one integer-like or float-like `var_dump()` scalar line.
fn eval_var_dump_append_scalar(
    label: &[u8],
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_prefix(depth, is_reference, output);
    output.extend_from_slice(label);
    output.extend_from_slice(b"(");
    output.extend_from_slice(&values.string_bytes(value)?);
    output.extend_from_slice(b")\n");
    Ok(())
}

/// Appends one string `var_dump()` line while preserving raw PHP string bytes.
fn eval_var_dump_append_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let bytes = values.string_bytes(value)?;
    eval_var_dump_append_prefix(depth, is_reference, output);
    output.extend_from_slice(b"string(");
    output.extend_from_slice(bytes.len().to_string().as_bytes());
    output.extend_from_slice(b") \"");
    output.extend_from_slice(&bytes);
    output.extend_from_slice(b"\"\n");
    Ok(())
}

/// Appends one boolean `var_dump()` line from PHP truthiness.
fn eval_var_dump_append_bool(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_prefix(depth, is_reference, output);
    if values.truthy(value)? {
        output.extend_from_slice(b"bool(true)\n");
    } else {
        output.extend_from_slice(b"bool(false)\n");
    }
    Ok(())
}

/// Appends one array shell and recursively emits foreach-visible entries.
fn eval_var_dump_append_array(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    arrays_seen: &mut Vec<usize>,
    objects_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        eval_var_dump_append_prefix(depth, is_reference, output);
        output.extend_from_slice(b"*RECURSION*\n");
        return Ok(());
    }

    arrays_seen.push(address);
    let len = values.array_len(value)?;
    eval_var_dump_append_prefix(depth, is_reference, output);
    output.extend_from_slice(b"array(");
    output.extend_from_slice(len.to_string().as_bytes());
    output.extend_from_slice(b") {\n");
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        eval_var_dump_append_array_key(key, values, depth + 1, output)?;
        eval_var_dump_append_value(
            element,
            context,
            values,
            depth + 1,
            false,
            arrays_seen,
            objects_seen,
            output,
        )?;
    }
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"}\n");
    arrays_seen.pop();
    Ok(())
}

/// Appends one object shell and its collected properties.
fn eval_var_dump_append_object(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    arrays_seen: &mut Vec<usize>,
    objects_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let identity = eval_debug_object_identity(value, values);
    let object_key = identity.unwrap_or(value.as_ptr() as usize as u64) as usize;
    if objects_seen.contains(&object_key) {
        eval_var_dump_append_prefix(depth, is_reference, output);
        output.extend_from_slice(b"*RECURSION*\n");
        return Ok(());
    }

    objects_seen.push(object_key);
    let class_name = eval_debug_object_class_name(value, identity, context, values)?;
    let properties = eval_debug_object_properties(value, identity, &class_name, context, values)?;
    eval_var_dump_append_prefix(depth, is_reference, output);
    output.extend_from_slice(b"object(");
    output.extend_from_slice(class_name.as_bytes());
    output.extend_from_slice(b")#");
    output.extend_from_slice(object_key.to_string().as_bytes());
    output.extend_from_slice(b" (");
    output.extend_from_slice(properties.len().to_string().as_bytes());
    output.extend_from_slice(b") {\n");
    for property in &properties {
        eval_var_dump_append_object_key(property, depth + 1, output);
        eval_var_dump_append_value(
            property.value,
            context,
            values,
            depth + 1,
            property.is_reference,
            arrays_seen,
            objects_seen,
            output,
        )?;
    }
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"}\n");
    objects_seen.pop();
    Ok(())
}

/// Appends one array key line for an indexed or associative `var_dump()` entry.
fn eval_var_dump_append_array_key(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"[");
    match values.type_tag(key)? {
        EVAL_TAG_STRING => {
            output.extend_from_slice(b"\"");
            output.extend_from_slice(&values.string_bytes(key)?);
            output.extend_from_slice(b"\"");
        }
        _ => output.extend_from_slice(&values.string_bytes(key)?),
    }
    output.extend_from_slice(b"]=>\n");
    Ok(())
}

/// Appends one object property key line for `var_dump()`.
fn eval_var_dump_append_object_key(
    property: &EvalDebugObjectProperty,
    depth: usize,
    output: &mut Vec<u8>,
) {
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"[\"");
    output.extend_from_slice(property.name.as_bytes());
    output.extend_from_slice(b"\"");
    match &property.visibility.kind {
        EvalDebugPropertyVisibilityKind::Public => {}
        EvalDebugPropertyVisibilityKind::Protected => output.extend_from_slice(b":protected"),
        EvalDebugPropertyVisibilityKind::Private(class_name) => {
            output.extend_from_slice(b":\"");
            output.extend_from_slice(class_name.as_bytes());
            output.extend_from_slice(b"\":private");
        }
    }
    output.extend_from_slice(b"]=>\n");
}

/// Appends one `var_dump()` line prefix, including a reference marker when needed.
fn eval_var_dump_append_prefix(depth: usize, is_reference: bool, output: &mut Vec<u8>) {
    eval_var_dump_append_indent(depth, output);
    if is_reference {
        output.extend_from_slice(b"&");
    }
}

/// Appends the two-space indentation used by PHP `var_dump()` arrays and objects.
fn eval_var_dump_append_indent(depth: usize, output: &mut Vec<u8>) {
    for _ in 0..depth {
        output.extend_from_slice(b"  ");
    }
}

/// Appends one value to a `print_r()` byte buffer.
fn eval_print_r_append_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    objects_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            eval_print_r_append_array(value, context, values, depth, arrays_seen, objects_seen, output)
        }
        EVAL_TAG_OBJECT => {
            eval_print_r_append_object(value, context, values, depth, arrays_seen, objects_seen, output)
        }
        _ => {
            output.extend_from_slice(&values.string_bytes(value)?);
            Ok(())
        }
    }
}

/// Appends one array in PHP `print_r()` style.
fn eval_print_r_append_array(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    objects_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        output.extend_from_slice(b"*RECURSION*");
        return Ok(());
    }
    arrays_seen.push(address);
    output.extend_from_slice(b"Array\n");
    eval_print_r_append_indent(depth, output);
    output.extend_from_slice(b"(\n");
    let len = values.array_len(value)?;
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        eval_print_r_append_indent(depth + 1, output);
        output.extend_from_slice(b"[");
        output.extend_from_slice(&values.string_bytes(key)?);
        output.extend_from_slice(b"] => ");
        eval_print_r_append_value(
            element,
            context,
            values,
            depth + 1,
            arrays_seen,
            objects_seen,
            output,
        )?;
        output.extend_from_slice(b"\n");
    }
    eval_print_r_append_indent(depth, output);
    output.extend_from_slice(b")\n");
    arrays_seen.pop();
    Ok(())
}

/// Appends one object in PHP `print_r()` style.
fn eval_print_r_append_object(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    objects_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let identity = eval_debug_object_identity(value, values);
    let object_key = identity.unwrap_or(value.as_ptr() as usize as u64) as usize;
    if objects_seen.contains(&object_key) {
        output.extend_from_slice(b"*RECURSION*");
        return Ok(());
    }
    objects_seen.push(object_key);
    let class_name = eval_debug_object_class_name(value, identity, context, values)?;
    let properties = eval_debug_object_properties(value, identity, &class_name, context, values)?;
    output.extend_from_slice(class_name.as_bytes());
    output.extend_from_slice(b" Object\n");
    eval_print_r_append_indent(depth, output);
    output.extend_from_slice(b"(\n");
    for property in &properties {
        eval_print_r_append_indent(depth + 1, output);
        eval_print_r_append_object_key(property, output);
        output.extend_from_slice(b" => ");
        eval_print_r_append_value(
            property.value,
            context,
            values,
            depth + 1,
            arrays_seen,
            objects_seen,
            output,
        )?;
        output.extend_from_slice(b"\n");
    }
    eval_print_r_append_indent(depth, output);
    output.extend_from_slice(b")\n");
    objects_seen.pop();
    Ok(())
}

/// Appends one object property key for `print_r()`.
fn eval_print_r_append_object_key(property: &EvalDebugObjectProperty, output: &mut Vec<u8>) {
    output.extend_from_slice(b"[");
    output.extend_from_slice(property.name.as_bytes());
    match &property.visibility.kind {
        EvalDebugPropertyVisibilityKind::Public => {}
        EvalDebugPropertyVisibilityKind::Protected => output.extend_from_slice(b":protected"),
        EvalDebugPropertyVisibilityKind::Private(class_name) => {
            output.extend_from_slice(b":");
            output.extend_from_slice(class_name.as_bytes());
            output.extend_from_slice(b":private");
        }
    }
    output.extend_from_slice(b"]");
}

/// Appends the four-space indentation used by PHP `print_r()`.
fn eval_print_r_append_indent(depth: usize, output: &mut Vec<u8>) {
    for _ in 0..depth {
        output.extend_from_slice(b"    ");
    }
}

/// Returns an object identity without turning non-object-like values into fatals.
fn eval_debug_object_identity(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Option<u64> {
    values.object_identity(value).ok()
}

/// Resolves the PHP-visible class name for one object value.
fn eval_debug_object_class_name(
    value: RuntimeCellHandle,
    identity: Option<u64>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if let Some(identity) = identity {
        if let Some(class) = context.dynamic_object_class(identity) {
            return Ok(class.name().trim_start_matches('\\').to_string());
        }
    }
    let class_name = values.object_class_name(value)?;
    let bytes = values.string_bytes(class_name)?;
    values.release(class_name)?;
    String::from_utf8(trim_leading_namespace_separator(&bytes).to_vec())
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Collects object properties visible to debug-output rendering.
fn eval_debug_object_properties(
    object: RuntimeCellHandle,
    identity: Option<u64>,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalDebugObjectProperty>, EvalStatus> {
    if let Some(identity) = identity {
        if context.dynamic_object_class(identity).is_some() {
            return eval_debug_dynamic_object_properties(object, identity, class_name, context, values);
        }
    }
    eval_debug_public_object_properties(object, values)
}

/// Collects eval-declared object properties plus public dynamic properties.
fn eval_debug_dynamic_object_properties(
    object: RuntimeCellHandle,
    identity: u64,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalDebugObjectProperty>, EvalStatus> {
    let mut properties = Vec::new();
    let mut storage_keys = HashSet::new();
    let mut emitted_public_names = HashSet::new();

    for class in context.class_chain(class_name) {
        for property in class.properties() {
            if property.is_static() {
                continue;
            }
            let storage_name = eval_instance_property_storage_name(class.name(), property);
            storage_keys.insert(storage_name.clone());
            if !property.is_virtual()
                && !context.dynamic_property_is_initialized(identity, &storage_name)
            {
                continue;
            }
            let alias = context.dynamic_property_alias(identity, &storage_name).cloned();
            let value = match &alias {
                Some(target) => eval_reference_target_value(target, context, values)?,
                None => values.property_get(object, &storage_name)?,
            };
            if property.visibility() == EvalVisibility::Public {
                emitted_public_names.insert(property.name().to_string());
            }
            properties.push(EvalDebugObjectProperty {
                name: property.name().to_string(),
                visibility: eval_debug_property_visibility(class.name(), property.visibility()),
                value,
                is_reference: alias.is_some(),
            });
        }
    }

    eval_debug_append_dynamic_public_properties(
        object,
        &storage_keys,
        &emitted_public_names,
        &mut properties,
        values,
    )?;
    Ok(properties)
}

/// Appends dynamic public properties stored directly on an eval object.
fn eval_debug_append_dynamic_public_properties(
    object: RuntimeCellHandle,
    storage_keys: &HashSet<String>,
    emitted_public_names: &HashSet<String>,
    properties: &mut Vec<EvalDebugObjectProperty>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key)?;
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        if key_name.contains('\0')
            || storage_keys.contains(&key_name)
            || emitted_public_names.contains(&key_name)
        {
            continue;
        }
        let value = values.property_get(object, &key_name)?;
        properties.push(EvalDebugObjectProperty {
            name: key_name,
            visibility: EvalDebugPropertyVisibility {
                kind: EvalDebugPropertyVisibilityKind::Public,
            },
            value,
            is_reference: false,
        });
    }
    Ok(())
}

/// Collects public bridge-visible properties for non-eval runtime objects.
fn eval_debug_public_object_properties(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalDebugObjectProperty>, EvalStatus> {
    let property_count = values.object_property_len(object)?;
    let mut properties = Vec::with_capacity(property_count);
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key)?;
        values.release(key)?;
        let key_name = String::from_utf8(key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        let value = values.property_get(object, &key_name)?;
        properties.push(EvalDebugObjectProperty {
            name: key_name,
            visibility: EvalDebugPropertyVisibility {
                kind: EvalDebugPropertyVisibilityKind::Public,
            },
            value,
            is_reference: false,
        });
    }
    Ok(properties)
}

/// Converts eval visibility metadata into debug-output key metadata.
fn eval_debug_property_visibility(
    declaring_class: &str,
    visibility: EvalVisibility,
) -> EvalDebugPropertyVisibility {
    let kind = match visibility {
        EvalVisibility::Public => EvalDebugPropertyVisibilityKind::Public,
        EvalVisibility::Protected => EvalDebugPropertyVisibilityKind::Protected,
        EvalVisibility::Private => {
            EvalDebugPropertyVisibilityKind::Private(declaring_class.trim_start_matches('\\').to_string())
        }
    };
    EvalDebugPropertyVisibility { kind }
}

/// Removes a leading PHP namespace separator from a runtime class-name byte slice.
fn trim_leading_namespace_separator(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\\").unwrap_or(bytes)
}
