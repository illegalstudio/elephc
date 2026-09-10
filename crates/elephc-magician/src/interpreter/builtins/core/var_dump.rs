//! Purpose:
//! Eval registry entry and implementation for `var_dump`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The formatter preserves PHP scalar, array, object, reference, and recursion output shape.
//! - Shared debug object metadata traversal is owned by `print_r` and reused here.

use super::print_r::{
    eval_debug_object_class_name, eval_debug_object_identity, eval_debug_object_properties,
    EvalDebugObjectProperty, EvalDebugPropertyVisibilityKind,
};
use super::super::super::*;

eval_builtin! {
    contract: "var_dump",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates PHP `var_dump()` over one or more eval expressions and returns null.
pub(in crate::interpreter) fn eval_builtin_var_dump(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let arguments = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_call("var_dump", &arguments, context, scope, values)
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
    let result = values.echo(output);
    let cleanup = eval_release_value(context, values, output);
    result?;
    cleanup?;
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
            eval_var_dump_append_resource(value, context, values, depth, is_reference, output)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends one `var_dump()` resource line carrying its PHP resource id and type name.
///
/// The id is the runtime registry's, reached through the same `(int)` coercion
/// `get_resource_id()` uses, NOT the eval-local payload: those are different
/// numbers by construction (see `crate::stream_resources`). This line used to be
/// the constant `resource(0) of type (stream)`, so every eval `var_dump()` of a
/// stream printed the same wrong number regardless of how many were open.
///
/// The type name is likewise computed rather than baked in: PHP 8.5.6 renames a CLOSED
/// resource to `Unknown`, and `eval_resource_type_name` reads that state from whichever
/// of the two close representations applies (see
/// `crate::interpreter::builtins::scalars::eval_resource_is_closed`).
fn eval_var_dump_append_resource(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    is_reference: bool,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let type_name = eval_resource_type_name(value, context, values)?;
    let id = values.cast_int(value)?;
    eval_var_dump_append_prefix(depth, is_reference, output);
    output.extend_from_slice(b"resource(");
    output.extend_from_slice(&values.string_bytes(id)?);
    output.extend_from_slice(b") of type (");
    output.extend_from_slice(type_name.as_bytes());
    output.extend_from_slice(b")\n");
    Ok(())
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
