//! Purpose:
//! Prepares numeric-entity maps through protected, incremental host value reads.
//!
//! Called from:
//! - The shared invocation coordinator after all outer parameter coercions.
//!
//! Key details:
//! - Encoding resolution and map-length validation precede element conversion.
//! - Diagnostics run outside request-state borrows and may update later references.
//! - The selected encoding stays fixed while substitution is read after map callbacks.

use std::collections::HashSet;
use super::{Argument, Outcome, RuntimeBuiltinId, REQUEST, host::{Session, Status}};
use super::super::entities::{convert, map_error};

/// Converts an encoding-valid map one entry at a time before invoking the pure entity engine.
pub(super) unsafe fn invoke(operation: RuntimeBuiltinId, values: &[Argument], session: &mut Session)
    -> Result<Outcome, Status>
{
    let (Argument::String(input), Argument::Array(root, _)) = (&values[0], &values[1]) else { return Err(Status::Fatal); };
    let encoding = match values.get(2) {
        Some(Argument::String(bytes)) => Some(bytes.as_slice()),
        None | Some(Argument::Null) => None,
        _ => return Err(Status::Fatal),
    };
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).ok_or(Status::Fatal)?;
    let resolved = match REQUEST.with(|state| state.borrow_mut().resolve_encoding(encoding, contract.name, 3, "encoding")) {
        Ok(resolved) => resolved, Err(error) => return Ok(Outcome::error(error)),
    };
    if let Some(message) = resolved.deprecation {
        unsafe { session.diagnostic(8192, format!("{}(): {message}", contract.name).as_bytes())?; }
    }
    let count = unsafe { session.entry_count(*root)? };
    if count % 4 != 0 { return Ok(map_error(operation, "must have a multiple of 4 elements")); }
    let mut map = Vec::with_capacity(count);
    let mut cursor = 0;
    let mut seen = HashSet::from([cursor]);
    while unsafe { session.next_entry(1, &mut cursor)? } {
        if !seen.insert(cursor) || map.len() >= count { return Err(Status::Fatal); }
        let value = unsafe { session.entry_integer()? };
        unsafe { session.release_entry()?; }
        let Some(value) = value else {
            return Ok(map_error(operation, "must only be composed of values of type int"));
        };
        map.push(value);
    }
    if map.len() != count { return Err(Status::Fatal); }
    let substitution = REQUEST.with(|state| state.borrow().substitute());
    let hex = matches!(values.get(3), Some(Argument::Bool(true)));
    Ok(convert(operation, input, &map, resolved.encoding, substitution, hex))
}
