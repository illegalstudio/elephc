//! Purpose:
//! Resolves encoding-list arrays one copied host element at a time.
//!
//! Called from:
//! - The shared invocation coordinator after outer PHP parameter coercions.
//!
//! Key details:
//! - String conversions and owner release occur outside request-state borrows.
//! - Each auto entry observes the language active at its own conversion point.
//! - The first invalid entry stops iteration and prevents later Stringable side effects.

use std::collections::HashSet;
use elephc_builtin_contract::mbstring_abi::array::{ArrayGraph, Key, Value};
use crate::{encoding::EncodingListBuilder, error::MbError};
use super::{host::{Session, Status}, REQUEST};

/// Resolves ordered host values into a canonical-name graph without capturing host pointers.
pub(super) unsafe fn prepare(session: &mut Session, index: usize, operation: elephc_builtin_contract::RuntimeBuiltinId)
    -> Result<Result<Vec<u8>, MbError>, Status>
{
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).ok_or(Status::Fatal)?;
    let parameter = contract.params.get(index).ok_or(Status::Fatal)?;
    let mut builder = EncodingListBuilder::new(contract.name, index + 1, parameter.name);
    let mut cursor = 0;
    let mut seen = HashSet::from([cursor]);
    while unsafe { session.next_entry(index, &mut cursor)? } {
        if !seen.insert(cursor) { return Err(Status::Fatal); }
        let name = unsafe { session.entry_string()? };
        let parsed = REQUEST.with(|state| state.borrow().push_array_encoding(&mut builder, &name));
        unsafe { session.release_entry()?; }
        if let Err(error) = parsed { return Ok(Err(error)); }
    }
    let names = match builder.finish() { Ok(names) => names, Err(error) => return Ok(Err(error)) };
    let entries = names.into_iter().enumerate().map(|(index, encoding)|
        (Key::Int(index as i64), Value::String(encoding.name().as_bytes().to_vec()))).collect();
    Ok(Ok(ArrayGraph::new(0, vec![entries]).expect("canonical encoding names form a valid list").encode()))
}
