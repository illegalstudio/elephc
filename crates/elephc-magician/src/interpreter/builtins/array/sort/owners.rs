//! Purpose:
//! Balances owned array-sort entry reads and unfinished result containers.
//!
//! Called from:
//! - Standard eval sort, key sort and shuffle builders.
//!
//! Key details:
//! - Entry keys and values each carry the lease returned by their runtime read.
//! - Comparisons and writes borrow those leases; cleanup runs on success and failure.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Captures entry leases in fixed root slots before fallible sort-key conversion runs.
pub(super) fn collect<V: RuntimeValueOps>(
    array: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut V,
    mut key: impl FnMut(RuntimeCellHandle, RuntimeCellHandle, &mut V) -> Result<EvalArraySortKey, EvalStatus>,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let empty = RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed();
    let owners = (0..len).map(|_| (RuntimeOwnerSlot::new(empty), RuntimeOwnerSlot::new(empty))).collect::<Vec<_>>();
    with_runtime_owners(&owners, || {
        let collected = (|| {
            let mut keys = Vec::with_capacity(len);
            for (position, (source_key, value)) in owners.iter().enumerate() {
                source_key.initialize(values.array_iter_key(array, position)?);
                value.initialize(values.array_get(array, source_key.borrowed())?);
                keys.push(key(source_key.borrowed(), value.borrowed(), values)?);
            }
            Ok(keys)
        })();
        match collected {
            Ok(keys) => Ok(keys.into_iter().zip(&owners).map(|(sort_key, (source_key, value))|
                EvalArraySortEntry { sort_key, source_key: source_key.take(), value: value.take() }).collect()),
            Err(status) => {
                let _ = finish_eval_argument_values(Err(status),
                    owners.iter().flat_map(|(key, value)| [value.take(), key.take()]), context, values);
                Err(status)
            }
        }
    })
}

/// Keeps every captured entry rooted until ordering and result construction finish.
pub(super) fn with_entries<V: RuntimeValueOps>(
    mut entries: Vec<EvalArraySortEntry>, context: &mut ElephcEvalContext, values: &mut V,
    consume: impl FnOnce(&mut [EvalArraySortEntry], &mut ElephcEvalContext, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let owners = entries.iter().flat_map(|entry| [entry.source_key, entry.value])
        .map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    let result = with_runtime_owners(&owners, || consume(&mut entries, context, values));
    finish_eval_argument_values(result, owners.iter().map(RuntimeOwnerSlot::take), context, values)
}

/// Publishes borrowed entries into a fresh unique container and retires partial output on failure.
pub(super) fn build_result(
    entries: &[EvalArraySortEntry], preserve_keys: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = if preserve_keys { values.assoc_new(entries.len())? } else { values.array_new(entries.len())? };
    let result = RuntimeOwnerSlot::new(result);
    with_runtime_owners(&result, || {
        let populated = (|| {
            for (index, entry) in entries.iter().enumerate() {
                if preserve_keys {
                    values.array_set(result.borrowed(), entry.source_key, entry.value)?;
                } else {
                    let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
                    let written = values.array_set(result.borrowed(), key, entry.value);
                    let released = release_expr_result(key, context, values);
                    written.and(released)?;
                }
            }
            Ok(())
        })();
        match populated {
            Ok(()) => Ok(result.take()),
            Err(status) => finish_eval_argument_values(Err(status), [result.take()], context, values),
        }
    })
}
