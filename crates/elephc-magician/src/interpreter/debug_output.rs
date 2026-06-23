//! Purpose:
//! Implements eval-side debug output builtins such as `print_r()` and `var_dump()`.
//!
//! Called from:
//! - `crate::interpreter::eval_positional_expr_call()` for debug-output builtin dispatch.
//!
//! Key details:
//! - Output formatting walks runtime arrays and scalar values only through `RuntimeValueOps`.
//! - The builtins either echo output directly or return captured string output according to PHP flags.

use super::*;

/// Evaluates PHP `print_r()` over one eval expression.
pub(super) fn eval_builtin_print_r(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_print_r_result(value, values)
}

/// Emits one eval value using elephc's supported `print_r()` output shape.
pub(in crate::interpreter) fn eval_print_r_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let output = values.string_bytes_value(b"Array\n")?;
        values.echo(output)?;
    } else {
        values.echo(value)?;
    }
    values.bool_value(true)
}

/// Evaluates PHP `var_dump()` over one eval expression and returns null.
pub(super) fn eval_builtin_var_dump(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_var_dump_result(value, values)
}

/// Emits one eval value using PHP-style `var_dump()` debug formatting.
pub(in crate::interpreter) fn eval_var_dump_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut output = Vec::new();
    let mut arrays_seen = Vec::new();
    eval_var_dump_append_value(value, values, 0, &mut arrays_seen, &mut output)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.null()
}

/// Appends one value and its nested array entries to a `var_dump()` byte buffer.
fn eval_var_dump_append_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => eval_var_dump_append_scalar(b"int", value, values, depth, output),
        EVAL_TAG_STRING => eval_var_dump_append_string(value, values, depth, output),
        EVAL_TAG_FLOAT => eval_var_dump_append_scalar(b"float", value, values, depth, output),
        EVAL_TAG_BOOL => eval_var_dump_append_bool(value, values, depth, output),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            eval_var_dump_append_array(value, values, depth, arrays_seen, output)
        }
        EVAL_TAG_OBJECT => {
            eval_var_dump_append_indent(depth, output);
            output.extend_from_slice(b"object(Object)\n");
            Ok(())
        }
        EVAL_TAG_NULL => {
            eval_var_dump_append_indent(depth, output);
            output.extend_from_slice(b"NULL\n");
            Ok(())
        }
        EVAL_TAG_RESOURCE => {
            eval_var_dump_append_indent(depth, output);
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
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_indent(depth, output);
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
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let bytes = values.string_bytes(value)?;
    eval_var_dump_append_indent(depth, output);
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
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_indent(depth, output);
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
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        eval_var_dump_append_indent(depth, output);
        output.extend_from_slice(b"*RECURSION*\n");
        return Ok(());
    }

    arrays_seen.push(address);
    let len = values.array_len(value)?;
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"array(");
    output.extend_from_slice(len.to_string().as_bytes());
    output.extend_from_slice(b") {\n");
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        eval_var_dump_append_key(key, values, depth + 1, output)?;
        eval_var_dump_append_value(element, values, depth + 1, arrays_seen, output)?;
    }
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"}\n");
    arrays_seen.pop();
    Ok(())
}

/// Appends one array key line for an indexed or associative `var_dump()` entry.
fn eval_var_dump_append_key(
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

/// Appends the two-space indentation used by PHP `var_dump()` arrays.
fn eval_var_dump_append_indent(depth: usize, output: &mut Vec<u8>) {
    for _ in 0..depth {
        output.extend_from_slice(b"  ");
    }
}
