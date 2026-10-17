//! Purpose:
//! Encodes eval objects and enum cases through their PHP JSON serialization rules.
//!
//! Called from:
//! - The parent encoder's object value dispatch.
//!
//! Key details:
//! - Enum jsonSerialize() takes precedence over backing scalars and the unit enum error.
//! - Callback results and backing property reads are released after recursive encoding.
//! - Serializer recursion uses object identity across distinct boxed handles.

use super::*;

/// Encodes enum overrides before falling back to ordinary public object properties.
pub(super) fn eval_json_encode_append_object(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let is_enum = match dynamic_object_is_a(value, "UnitEnum", false, context, values)? {
        Some(is_enum) => is_enum,
        None => {
            // Native enum metadata does not expose the implicit UnitEnum interface.
            let class_name = runtime_object_class_name(value, values)?;
            values.enum_exists(&class_name)?
        }
    };
    if is_enum {
        let is_serializable = dynamic_object_is_a(value, "JsonSerializable", false, context, values)?
            .map_or_else(|| values.object_is_a(value, "JsonSerializable", false), Ok)?;
        if is_serializable {
            return append_enum_serialization(
                value, context, values, flags, depth_limit, depth, arrays_seen, error, output,
            );
        }
        let is_backed = match dynamic_object_is_a(value, "BackedEnum", false, context, values)? {
            Some(is_backed) => is_backed,
            // Native enum value slots use declared property initialization metadata.
            None => values.property_is_initialized(value, "value")?,
        };
        if is_backed {
            let backing = eval_property_get_result(value, "value", context, values)?;
            let encoded = eval_json_encode_append(
                backing, context, values, flags, depth_limit, depth, arrays_seen, error, output,
            );
            let released = release_encoded_value(backing, context, values);
            return encoded.and(released);
        }
        *error = Some(EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_NON_BACKED_ENUM,
            message: "Non-backed enums have no default serialization",
        });
        output.push(b'0');
        return Ok(());
    }
    append_properties(
        value, context, values, flags, depth_limit, depth, arrays_seen, error, output,
    )
}

/// Runs an enum serializer once and encodes its owned result without adding a container level.
fn append_enum_serialization(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(value)? as usize;
    if arrays_seen.contains(&identity) {
        *error = Some(EvalJsonEncodeError { code: 6, message: "Recursion detected" });
        output.extend_from_slice(b"null");
        return Ok(());
    }
    arrays_seen.push(identity);
    let result = eval_method_call_result(value, "jsonSerialize", Vec::new(), context, values);
    let encoded = match result {
        Ok(result) => {
            let encoded = (|| {
                if values.type_tag(result)? == EVAL_TAG_OBJECT
                    && values.object_identity(result)? as usize == identity
                {
                    // PHP exposes the properties when jsonSerialize() returns its receiver.
                    let native_receiver = dynamic_object_is_a(
                        result, "UnitEnum", false, context, values,
                    )?.is_none();
                    arrays_seen.pop();
                    let encoded = if native_receiver {
                        append_native_enum_properties(
                            result, context, values, flags, depth_limit, depth, arrays_seen, error, output,
                        )
                    } else {
                        append_properties(
                            result, context, values, flags, depth_limit, depth, arrays_seen, error, output,
                        )
                    };
                    arrays_seen.push(identity);
                    encoded
                } else {
                    eval_json_encode_append(
                        result, context, values, flags, depth_limit, depth, arrays_seen, error, output,
                    )
                }
            })();
            let released = release_encoded_value(result, context, values);
            encoded.and(released)
        }
        Err(status) => Err(status),
    };
    arrays_seen.pop();
    encoded
}

/// Reads native enum properties explicitly because its bridge iterator covers only stdClass.
fn append_native_enum_properties(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let count = if values.property_is_initialized(value, "value")? { 2 } else { 1 };
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let encoded = (|| {
        let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
        output.push(b'{');
        if pretty { output.push(b'\n'); }
        for (position, name) in ["name", "value"].iter().take(count).enumerate() {
            if position > 0 {
                output.push(b',');
                if pretty { output.push(b'\n'); }
            }
            if pretty { eval_json_encode_pretty_indent(output, depth + 1); }
            output.push(b'"');
            output.extend_from_slice(name.as_bytes());
            output.push(b'"');
            eval_json_encode_append_colon(flags, output);
            let property = eval_property_get_result(value, name, context, values)?;
            let encoded = eval_json_encode_append(
                property, context, values, flags, depth_limit, depth + 1, arrays_seen, error, output,
            );
            let released = release_encoded_value(property, context, values);
            encoded.and(released)?;
        }
        if pretty {
            output.push(b'\n');
            eval_json_encode_pretty_indent(output, depth);
        }
        output.push(b'}');
        Ok(())
    })();
    arrays_seen.pop();
    encoded
}

/// Releases only an owned callback/property result, including after encoding fails.
fn release_encoded_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if value.is_borrowed() { Ok(()) } else { eval_release_value(context, values, value) }
}

/// Appends public properties when no enum override applies, including a serializer returning itself.
fn append_properties(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(b'{');
    let len = values.object_property_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.object_property_iter_key(value, position)?;
        let key_bytes = values.string_bytes(key)?;
        eval_json_encode_append_string(
            &key_bytes,
            flags & !EVAL_JSON_NUMERIC_CHECK,
            EvalJsonStringPosition::Key,
            error,
            output,
        )?;
        eval_json_encode_append_colon(flags, output);
        let property = std::str::from_utf8(&key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        let element = values.property_get(value, property)?;
        eval_json_encode_append(
            element,
            context,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(b'}');
    arrays_seen.pop();
    Ok(())
}
