//! Purpose:
//! Builds splice results while balancing every owned key and element read.
//!
//! Called from:
//! - Evaluated and direct `array_splice` adapters.
//!
//! Key details:
//! - Existing string keys survive; numeric and inserted keys are reindexed.
//! - Fresh output and extracted entries remain rooted across fallible runtime calls.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use std::ops::Range;

/// Copies selected ranges around optional replacement values into one fresh unique array.
pub(super) fn build(
    source: RuntimeCellHandle, prefix: Range<usize>, inserted: Option<RuntimeCellHandle>, suffix: Range<usize>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let inserted_array = inserted.map(|value| values.type_tag(value)
        .map(|tag| matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC))).transpose()?.unwrap_or(false);
    let inserted_len = match inserted {
        Some(value) if inserted_array => values.array_len(value)?,
        Some(_) => 1,
        None => 0,
    };
    let mut string_keys = false;
    for position in prefix.clone().chain(suffix.clone()) {
        let key = values.array_iter_key(source, position)?;
        let kind = values.type_tag(key);
        let kind = finish_read(kind, [key], context, values)?;
        string_keys |= kind == EVAL_TAG_STRING;
    }
    let capacity = prefix.len().checked_add(suffix.len()).and_then(|len| len.checked_add(inserted_len))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let output = RuntimeOwnerSlot::new(if string_keys { values.assoc_new(capacity)? } else { values.array_new(capacity)? });
    with_runtime_owners(&output, || {
        let mut next_int = 0_i64;
        let populated = (|| {
            for position in prefix {
                copy_entry(source, position, output.borrowed(), true, &mut next_int, context, values)?;
            }
            if let Some(inserted) = inserted {
                if inserted_array {
                    for position in 0..inserted_len {
                        copy_entry(inserted, position, output.borrowed(), false, &mut next_int, context, values)?;
                    }
                } else {
                    append(output.borrowed(), None, inserted, &mut next_int, context, values)?;
                }
            }
            for position in suffix {
                copy_entry(source, position, output.borrowed(), true, &mut next_int, context, values)?;
            }
            Ok(())
        })();
        match populated {
            Ok(()) => Ok(output.take()),
            Err(status) => finish_eval_argument_values(Err(status), [output.take()], context, values),
        }
    })
}

/// Borrows one owned source entry during insertion and retires both read leases afterwards.
fn copy_entry(
    source: RuntimeCellHandle, position: usize, output: RuntimeCellHandle, preserve_string: bool,
    next_int: &mut i64, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let key = RuntimeOwnerSlot::new(values.array_iter_key(source, position)?);
    with_runtime_owners(&key, || {
        let result = (|| {
            let value = RuntimeOwnerSlot::new(values.array_get(source, key.borrowed())?);
            with_runtime_owners(&value, || {
                let result = (|| {
                    let keep = preserve_string && values.type_tag(key.borrowed())? == EVAL_TAG_STRING;
                    append(output, keep.then(|| key.borrowed()), value.borrowed(), next_int, context, values)
                })();
                finish_read(result, [value.take()], context, values)
            })
        })();
        finish_read(result, [key.take()], context, values)
    })
}

/// Retains a borrowed entry in output and releases any generated numeric key immediately.
fn append(
    output: RuntimeCellHandle, key: Option<RuntimeCellHandle>, value: RuntimeCellHandle,
    next_int: &mut i64, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(key) = key { return values.array_set(output, key, value).map(|_| ()); }
    let key = RuntimeOwnerSlot::new(values.int(*next_int)?);
    with_runtime_owners(&key, || {
        let written = (|| {
            *next_int = next_int.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            values.array_set(output, key.borrowed(), value).map(|_| ())
        })();
        finish_read(written, [key.take()], context, values)
    })
}

/// Retires scalar-operation input leases while preserving the original failure priority.
fn finish_read<T>(
    result: Result<T, EvalStatus>, owners: impl IntoIterator<Item = RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let owners = owners.into_iter().map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    with_runtime_owners(&owners, || {
        let mut cleanup = Ok(());
        for owner in &owners {
            let released = release_expr_result(owner.take(), context, values);
            if cleanup.is_ok() { cleanup = released; }
        }
        result.and_then(|result| cleanup.map(|()| result))
    })
}
