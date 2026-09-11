//! Purpose:
//! Dispatches numeric entities over validated map snapshots and shared character codecs.
//!
//! Called from:
//! - The direct C ABI and protected host invocation coordinator.
//!
//! Key details:
//! - Encoding validation precedes map length and element validation.
//! - Public host calls deliver map diagnostics incrementally before converting later references.

use super::*;
use crate::{coercion::Input, encoding::{Encoding, Substitute}};
use elephc_builtin_contract::mbstring_abi::array::Value;

/// Converts a map-validation failure into the current operation's complete PHP ValueError.
pub(super) fn map_error(operation: RuntimeBuiltinId, message: &str) -> Outcome {
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("entity contract");
    Outcome::error(MbError::argument(contract.name, 2, "map", message))
}

/// Runs one entity transform after its map and encoding are prepared without further callbacks.
pub(super) fn convert(operation: RuntimeBuiltinId, input: &[u8], map: &[i64], encoding: Encoding,
    substitute: Substitute, hex: bool) -> Outcome
{
    Outcome::string(match operation {
        RuntimeBuiltinId::MbEncodeNumericentity => crate::text::encode_numericentity(input, map, encoding, substitute, hex),
        RuntimeBuiltinId::MbDecodeNumericentity => crate::text::decode_numericentity(input, map, encoding, substitute),
        _ => return Outcome::unsupported(),
    })
}

/// Runs the callback-free wire surface, preserving ordered diagnostics in the owned result.
pub(super) fn dispatch(operation: RuntimeBuiltinId, args: &Arguments<'_>, state: &mut State) -> Outcome {
    let resolved = match state.resolve_encoding(args.nullable_string(2), args.contract.name, 3, "encoding") {
        Ok(resolved) => resolved, Err(error) => return Outcome::error(error),
    };
    let mut diagnostics = resolved.deprecation.map_or_else(Vec::new, |message|
        format!("Deprecated: {}(): {message}\n", args.contract.name).into_bytes());
    let graph = args.array(1).expect("validated map array");
    let entries = &graph.arrays()[graph.root()];
    let mut output = if entries.len() % 4 != 0 {
        map_error(operation, "must have a multiple of 4 elements")
    } else {
        let mut map = Vec::with_capacity(entries.len());
        for (_, value) in entries {
            let input = match value {
                Value::Null => Input::Null, Value::Bool(value) => Input::Bool(*value),
                Value::Int(value) => Input::Int(*value), Value::Float(bits) => Input::Float(*bits),
                Value::String(bytes) => Input::String(bytes),
                Value::Array(_) | Value::Unsupported => Input::Array,
            };
            let (value, messages) = crate::coercion::entity_map::integer(input);
            for message in messages {
                diagnostics.extend_from_slice(if message.level == 8192 { b"Deprecated: " } else { b"Warning: " });
                diagnostics.extend_from_slice(&message.message);
                diagnostics.push(b'\n');
            }
            let Some(value) = value else {
                let mut error = map_error(operation, "must only be composed of values of type int");
                error.diagnostics = diagnostics;
                return error;
            };
            map.push(value);
        }
        convert(operation, args.string(0), &map, resolved.encoding, state.substitute(),
            operation == RuntimeBuiltinId::MbEncodeNumericentity && args.boolean(3))
    };
    output.diagnostics = diagnostics;
    output
}
